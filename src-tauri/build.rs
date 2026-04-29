// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// Build scripts use panic! to abort with a clear message — this is standard
// practice and not a runtime concern.
#![allow(clippy::panic, clippy::expect_used, clippy::unwrap_used)]

fn main() {
    emit_build_info();

    // ── macOS / Linux: increase main-thread stack size (binary only) ─────
    //
    // The Tauri `run()` function has an enormous stack frame because
    // `generate_handler!` expands ~150 command handlers inline.  macOS
    // defaults to 8 MB which is insufficient.
    let t_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if t_os == "macos" {
        println!("cargo:rustc-link-arg-bins=-Wl,-stack_size,0x2000000");
        // Weak-link WidgetKit so widget_reload::reload_all_widgets() can call
        // WGWidgetCenter at runtime.  Weak linking avoids crashes on macOS < 14
        // where the framework doesn't exist — the class lookup returns nil instead.
        println!("cargo:rustc-link-lib=framework=WidgetKit");
        println!("cargo:rustc-link-arg-bins=-Wl,-weak_framework,WidgetKit");
    } else if t_os == "linux" {
        println!("cargo:rustc-link-arg-bins=-Wl,-z,stacksize=33554432");
    }

    // ── Vulkan SDK: setup on Windows and Linux ────────────────────────────
    #[cfg(target_os = "windows")]
    setup_vulkan_sdk_windows();

    #[cfg(target_os = "linux")]
    setup_vulkan_sdk_linux();

    // ── Windows app manifest ─────────────────────────────────────────────
    #[cfg(target_os = "windows")]
    {
        let manifest = include_str!("./manifest.xml");
        let windows = tauri_build::WindowsAttributes::new().app_manifest(manifest);
        let attrs = tauri_build::Attributes::new().windows_attributes(windows);
        tauri_build::try_build(attrs).expect("failed to run tauri build script");
    }

    #[cfg(not(target_os = "windows"))]
    {
        tauri_build::build();
    }
}

// ── Build-info env vars ───────────────────────────────────────────────────────
//
// Bake git identity into the binary so the About page can show a stable
// commit/tag pair regardless of the version field in tauri.conf.json.
// The version string is reserved for the Tauri updater's SemVer ordering;
// these values are what humans look at.
//
// Failures are non-fatal: if git is unavailable (e.g. building from a tarball)
// the env vars come out empty and the runtime falls back to the package
// version. CI shallow checkouts work fine because the workflows already do
// fetch-depth: 0.

fn emit_build_info() {
    // Re-run when the current ref changes so cached builds pick up new commits.
    println!("cargo:rerun-if-changed=../.git/HEAD");
    println!("cargo:rerun-if-changed=../.git/refs");

    let tag = git(&["describe", "--tags", "--always", "--dirty"]).unwrap_or_default();
    let commit = git(&["rev-parse", "HEAD"]).unwrap_or_default();
    let branch = git(&["rev-parse", "--abbrev-ref", "HEAD"]).unwrap_or_default();
    let date = git(&["log", "-1", "--format=%cI"]).unwrap_or_default();

    println!("cargo:rustc-env=BUILD_INFO_TAG={tag}");
    println!("cargo:rustc-env=BUILD_INFO_COMMIT={commit}");
    println!("cargo:rustc-env=BUILD_INFO_BRANCH={branch}");
    println!("cargo:rustc-env=BUILD_INFO_DATE={date}");
}

fn git(args: &[&str]) -> Option<String> {
    let output = std::process::Command::new("git")
        .args(args)
        // Run from the repo root (one above src-tauri) so describe finds tags.
        .current_dir("..")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

// ── Vulkan SDK helpers (unchanged) ────────────────────────────────────────────

#[cfg(target_os = "windows")]
fn setup_vulkan_sdk_windows() {
    use std::path::Path;
    use std::process::Command;

    let vulkan_sdk_path =
        std::env::var("VULKAN_SDK").unwrap_or_else(|_| "C:\\VulkanSDK".to_string());

    if Path::new(&vulkan_sdk_path).exists() {
        println!(
            "cargo:warning=Vulkan SDK already installed at {}",
            vulkan_sdk_path
        );
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

    let install_status = Command::new(&installer_path).args(["/S"]).status();

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

    let _ = std::fs::remove_file(&installer_path);
}

#[cfg(target_os = "linux")]
fn setup_vulkan_sdk_linux() {
    use std::process::Command;

    let pkg_config_output = Command::new("pkg-config")
        .args(["--cflags", "--libs", "vulkan"])
        .output();

    if let Ok(output) = pkg_config_output {
        if output.status.success() {
            println!("cargo:warning=Vulkan SDK found via pkg-config");
            let libs_output = String::from_utf8_lossy(&output.stdout);
            println!("cargo:rustc-link-lib=vulkan");
            for token in libs_output.split_whitespace() {
                if let Some(path) = token.strip_prefix("-L") {
                    println!("cargo:rustc-link-search={}", path);
                }
            }
            return;
        }
    }

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
            println!(
                "cargo:warning=  Ubuntu/Debian: sudo apt-get install libvulkan-dev vulkan-tools"
            );
            println!(
                "cargo:warning=  Fedora/RHEL:   sudo dnf install vulkan-loader-devel vulkan-tools"
            );
            println!(
                "cargo:warning=  Arch:          sudo pacman -S vulkan-icd-loader vulkan-devel"
            );
            println!("cargo:rustc-link-lib=vulkan");
        }
    }
}
