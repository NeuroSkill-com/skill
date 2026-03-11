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
    // Only copy during release builds.  In dev mode the binary resolves espeak
    // data via the ESPEAK_DATA_PATH_DEV compile-time env (baked above), so no
    // copy is required.
    //
    // Skipping the copy in dev mode breaks an infinite rebuild loop that
    // otherwise occurs in `tauri dev`:
    //   build.rs copies → resources/espeak-ng-data/ changes
    //   → Tauri file watcher triggers cargo run
    //   → build.rs copies again → ...
    //
    // In dev mode we still create the (empty) directory so that Tauri's
    // resource-path validation in tauri.conf.json does not error on startup.
    let profile = std::env::var("PROFILE").unwrap_or_default();
    if profile == "release" {
        #[cfg(target_os = "macos")]
        bundle_espeak_data_macos();
        #[cfg(target_os = "linux")]
        bundle_espeak_data_linux();
        #[cfg(target_os = "windows")]
        bundle_espeak_data_windows();
    } else {
        std::fs::create_dir_all("resources/espeak-ng-data")
            .unwrap_or_else(|e| panic!("build.rs: create_dir_all(resources/espeak-ng-data): {e}"));
    }

    // ── Vulkan SDK: setup on Windows and Linux ────────────────────────────────
    #[cfg(target_os = "windows")]
    setup_vulkan_sdk_windows();
    
    #[cfg(target_os = "linux")]
    setup_vulkan_sdk_linux();

    // Ensure we're using proper toolchain for the MSVC target
    if cfg!(target_os = "windows") {
        println!("cargo:warning=Using Windows build configuration");
    }

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
        let archive = Path::new(&dir).join(lib);
        if !archive.exists() || !is_correct_platform_archive(&archive, &t_os) {
            if archive.exists() {
                eprintln!(
                    "build.rs: {} exists but is built for the wrong platform — rebuilding.",
                    archive.display()
                );
                let _ = std::fs::remove_file(&archive);
            }
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
    // Rebuild if missing OR if the cached archive was built for a different
    // OS (e.g. a Linux ELF archive committed to the repo and then used on macOS).
    if !Path::new(&local).exists()
        || !is_correct_platform_archive(Path::new(&local), &t_os)
    {
        if Path::new(&local).exists() {
            eprintln!(
                "build.rs: {local} exists but is built for the wrong platform — rebuilding."
            );
            // Remove the whole espeak-static dir so the build script starts clean.
            let stale_dir = if t_os == "windows" && t_env == "gnu" {
                "espeak-static-mingw"
            } else {
                "espeak-static"
            };
            let _ = std::fs::remove_dir_all(stale_dir);
        }
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

// ── Platform format guard ─────────────────────────────────────────────────────
//
// Returns true when `archive` is an object-file archive built for `t_os`.
//
// Detection strategy:
//   macOS  – run `lipo -info`; it prints architecture names for valid Mach-O
//            archives and exits non-zero for ELF ones.
//   Linux  – read the first 4 bytes of the first member; ELF magic is
//            7f 45 4c 46.  Mach-O LE magic is CE/CF FA ED FE.
//   other  – always return true (no check performed).
//
// The function is intentionally conservative: if the tool is absent or the
// archive cannot be opened, it returns `true` so we don't rebuild needlessly.
fn is_correct_platform_archive(archive: &Path, t_os: &str) -> bool {
    match t_os {
        "macos" => {
            // `lipo -info` exits 0 only for valid Mach-O fat/thin archives.
            Command::new("lipo")
                .args(["-info", &archive.to_string_lossy()])
                .output()
                .map(|o| o.status.success())
                .unwrap_or(true)   // lipo absent → assume OK
        }
        "linux" => {
            // Open the ar archive and peek at the first object's magic bytes.
            // ar format: "!<arch>\n" (8 bytes), then a sequence of:
            //   name(16) + date(12) + uid(6) + gid(6) + mode(8) + size(10) + "`\n"(2)
            //   = 60-byte header, followed by `size` bytes of data.
            // The first member in a GNU-format archive is the symbol table named
            // "/" — its data is an index, not an object file.  We skip it and
            // look at the first real object.
            fn first_obj_magic(archive: &Path) -> Option<[u8; 4]> {
                let data = std::fs::read(archive).ok()?;
                if data.len() < 8 || &data[..8] != b"!<arch>\n" {
                    return None;
                }
                let mut pos: usize = 8;
                loop {
                    if pos + 60 > data.len() { return None; }
                    let hdr = &data[pos..pos + 60];
                    let name = std::str::from_utf8(&hdr[..16]).ok()?.trim();
                    let size_str = std::str::from_utf8(&hdr[48..58]).ok()?.trim();
                    let size: usize = size_str.parse().ok()?;
                    let obj_start = pos + 60;
                    let obj_end   = obj_start + size;
                    // Skip GNU symbol table ("/") and extended name table ("//").
                    if name != "/" && name != "//" {
                        if obj_end > data.len() || obj_start + 4 > data.len() {
                            return None;
                        }
                        let mut magic = [0u8; 4];
                        magic.copy_from_slice(&data[obj_start..obj_start + 4]);
                        return Some(magic);
                    }
                    // Advance past this member (headers are 2-byte-aligned).
                    pos = obj_end + (size & 1);
                }
            }
            match first_obj_magic(archive) {
                Some(magic) => magic == [0x7f, b'E', b'L', b'F'],  // ELF magic
                None        => true,  // can't determine → assume OK
            }
        }
        _ => true,
    }
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

    // bash is available on all supported hosts: Linux, macOS, and MSYS2/MinGW
    // on Windows (the MinGW target is only reachable from an MSYS2 shell).
    let status = Command::new("bash")
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
        .find(|p| !p.contains("//") && Path::new(p.as_str()).is_dir())
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
        .find(|p| !p.is_empty() && Path::new(p.as_str()).is_dir())
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
        clear_readonly(path);
        std::fs::remove_file(path).ok();
    }
}

/// Clear the read-only attribute on `path` so a subsequent write or remove
/// does not fail with "Permission denied".
///
/// On Unix we set only the owner-write bit (0o200) rather than calling
/// `set_readonly(false)`, which would make the file world-writable.
/// On Windows `set_readonly(false)` is the correct cross-platform API.
fn clear_readonly(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = std::fs::metadata(path) {
            let mut perms = meta.permissions();
            perms.set_mode(perms.mode() | 0o200); // owner-write only
            std::fs::set_permissions(path, perms).ok();
        }
    }
    #[cfg(not(unix))]
    {
        if let Ok(meta) = std::fs::metadata(path) {
            let mut perms = meta.permissions();
            #[allow(clippy::permissions_set_readonly_false)]
            perms.set_readonly(false);
            std::fs::set_permissions(path, perms).ok();
        }
    }
}

#[cfg(target_os = "windows")]
fn setup_vulkan_sdk_windows() {
    // use std::fs;
    
    let vulkan_sdk_path = "C:\\VulkanSDK";
    
    // Check if Vulkan SDK is already installed
    if Path::new(vulkan_sdk_path).exists() {
        println!("cargo:warning=Vulkan SDK already installed at {}", vulkan_sdk_path);
        println!("cargo:rustc-link-search={}\\Lib", vulkan_sdk_path);
        println!("cargo:rustc-link-lib=vulkan-1");
        return;
    }
    
    println!("cargo:warning=Vulkan SDK not found. Installing...");
    
    let vulkan_version = "1.3.280";
    let installer_url = format!(
        "https://sdk.lunarg.com/sdk/download/{}/windows/VulkanSDK-{}-Installer.exe",
        vulkan_version, vulkan_version
    );
    
    let temp_dir = std::env::temp_dir();
    let installer_path = temp_dir.join("VulkanSDK-installer.exe");
    
    // Download Vulkan SDK installer
    println!("cargo:warning=Downloading Vulkan SDK from {}", installer_url);
    let download_status = Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-Command",
            &format!(
                "Invoke-WebRequest -Uri '{}' -OutFile '{}'",
                installer_url,
                installer_path.display()
            ),
        ])
        .status();
    
    match download_status {
        Ok(status) if status.success() => {
            println!("cargo:warning=Download successful");
        }
        _ => {
            println!("cargo:warning=Failed to download Vulkan SDK. Please install manually from https://vulkan.lunarg.com/sdk/home");
            return;
        }
    }
    
    // Run installer (silent mode)
    println!("cargo:warning=Running Vulkan SDK installer...");
    let install_status = Command::new(&installer_path)
        .args(["/S"])
        .status();
    
    match install_status {
        Ok(status) if status.success() => {
            println!("cargo:warning=Vulkan SDK installed successfully");
            println!("cargo:rustc-link-search={}\\Lib", vulkan_sdk_path);
            println!("cargo:rustc-link-lib=vulkan-1");
        }
        _ => {
            println!("cargo:warning=Failed to install Vulkan SDK. Please install manually from https://vulkan.lunarg.com/sdk/home");
        }
    }
    
    // Cleanup
    let _ = std::fs::remove_file(&installer_path);
}

#[cfg(target_os = "linux")]
fn setup_vulkan_sdk_linux() {
    // First, try to use system package manager
    println!("cargo:warning=Checking for Vulkan SDK...");
    
    let pkg_config_output = Command::new("pkg-config")
        .args(["--cflags", "--libs", "vulkan"])
        .output();
    
    if let Ok(output) = pkg_config_output {
        if output.status.success() {
            println!("cargo:warning=Vulkan SDK found via pkg-config");
            let libs_output = String::from_utf8_lossy(&output.stdout);
            println!("cargo:rustc-link-lib=vulkan");
            
            // Parse pkg-config output for library paths
            for token in libs_output.split_whitespace() {
                if let Some(path) = token.strip_prefix("-L") {
                    println!("cargo:rustc-link-search={}", path);
                }
            }
            return;
        }
    }
    
    // Vulkan SDK not found via pkg-config, try to install it
    println!("cargo:warning=Vulkan SDK not found. Attempting to install via apt...");
    
    let install_status = Command::new("sudo")
        .args(["apt-get", "install", "-y", "libvulkan-dev", "vulkan-tools"])
        .status();
    
    match install_status {
        Ok(status) if status.success() => {
            println!("cargo:warning=Vulkan SDK installed successfully");
            println!("cargo:rustc-link-search=/usr/lib");
            println!("cargo:rustc-link-search=/usr/lib/x86_64-linux-gnu");
            println!("cargo:rustc-link-lib=vulkan");
        }
        _ => {
            println!("cargo:warning=Failed to install Vulkan SDK via apt.");
            println!("cargo:warning=Please install manually:");
            println!("cargo:warning=  Ubuntu/Debian: sudo apt-get install libvulkan-dev vulkan-tools");
            println!("cargo:warning=  Fedora/RHEL:   sudo dnf install vulkan-loader-devel vulkan-tools");
            println!("cargo:warning=  Arch:          sudo pacman -S vulkan-icd-loader vulkan-devel");
            println!("cargo:rustc-link-lib=vulkan");
        }
    }
}