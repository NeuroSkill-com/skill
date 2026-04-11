// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// Build scripts use panic! to abort with a clear message — this is standard
// practice and not a runtime concern.
#![allow(clippy::panic, clippy::expect_used, clippy::unwrap_used)]

fn main() {
    // ── macOS / Linux: increase main-thread stack size (binary only) ─────
    //
    // The Tauri `run()` function has an enormous stack frame because
    // `generate_handler!` expands ~150 command handlers inline.  macOS
    // defaults to 8 MB which is insufficient.
    let t_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if t_os == "macos" {
        println!("cargo:rustc-link-arg-bins=-Wl,-stack_size,0x2000000");
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
