// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
//! Platform-specific launch-at-login (autostart) registration.
//!
//! Uses only Rust std — no additional crate required.
//!
//! | Platform | Mechanism                                                        |
//! |----------|------------------------------------------------------------------|
//! | macOS    | LaunchAgent plist in `~/Library/LaunchAgents/`                   |
//! | Linux    | XDG `.desktop` file in `~/.config/autostart/`                   |
//! | Windows  | `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` via `reg`  |
//!
//! The plist / desktop file / registry key is always named after `app_name`
//! (lowercased) so multiple Tauri apps on the same machine can coexist.

// ── Public API ────────────────────────────────────────────────────────────────

/// Returns `true` if launch-at-login is currently registered for this app.
pub fn is_enabled(app_name: &str) -> bool {
    #[cfg(target_os = "macos")]
    { macos::is_enabled(app_name) }
    #[cfg(target_os = "linux")]
    { linux::is_enabled(app_name) }
    #[cfg(target_os = "windows")]
    { windows::is_enabled(app_name) }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    { false }
}

/// Enable or disable launch-at-login.
///
/// Derives the executable path from [`std::env::current_exe`].
pub fn set_enabled(app_name: &str, enabled: bool) -> Result<(), String> {
    if enabled {
        let exe = std::env::current_exe()
            .map_err(|e| format!("cannot locate executable: {e}"))?
            .to_string_lossy()
            .to_string();
        enable(app_name, &exe)
    } else {
        disable(app_name)
    }
}

// ── Platform dispatch ─────────────────────────────────────────────────────────

fn enable(app_name: &str, exe: &str) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    return macos::enable(app_name, exe);
    #[cfg(target_os = "linux")]
    return linux::enable(app_name, exe);
    #[cfg(target_os = "windows")]
    return windows::enable(app_name, exe);
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    { let _ = (app_name, exe); Err("autostart not supported on this platform".into()) }
}

fn disable(app_name: &str) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    return macos::disable(app_name);
    #[cfg(target_os = "linux")]
    return linux::disable(app_name);
    #[cfg(target_os = "windows")]
    return windows::disable(app_name);
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    { let _ = app_name; Err("autostart not supported on this platform".into()) }
}

// ── macOS — LaunchAgent plist ─────────────────────────────────────────────────

#[cfg(target_os = "macos")]
mod macos {
    use std::path::PathBuf;
    use skill_constants::AUTOSTART_PLIST_LABEL_PREFIX;

    fn plist_path(app_name: &str) -> Option<PathBuf> {
        std::env::var("HOME").ok().map(|h| {
            PathBuf::from(h)
                .join("Library/LaunchAgents")
                .join(format!("{AUTOSTART_PLIST_LABEL_PREFIX}.{app_name}.loginitem.plist"))
        })
    }

    pub fn is_enabled(app_name: &str) -> bool {
        plist_path(app_name).map(|p| p.exists()).unwrap_or(false)
    }

    pub fn enable(app_name: &str, exe: &str) -> Result<(), String> {
        let path = plist_path(app_name).ok_or_else(|| "HOME not set".to_string())?;
        if let Some(parent) = path.parent() { let _ = std::fs::create_dir_all(parent); }
        let label = format!("{AUTOSTART_PLIST_LABEL_PREFIX}.{app_name}.loginitem");
        let plist = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
    "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{label}</string>
    <key>ProgramArguments</key>
    <array>
        <string>{exe}</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <false/>
    <key>ThrottleInterval</key>
    <integer>5</integer>
</dict>
</plist>
"#
        );
        std::fs::write(&path, plist)
            .map_err(|e| format!("failed to write LaunchAgent: {e}"))
    }

    pub fn disable(app_name: &str) -> Result<(), String> {
        let path = plist_path(app_name).ok_or_else(|| "HOME not set".to_string())?;
        if path.exists() {
            std::fs::remove_file(&path)
                .map_err(|e| format!("failed to remove LaunchAgent: {e}"))?;
        }
        Ok(())
    }
}

// ── Linux — XDG autostart .desktop file ──────────────────────────────────────

#[cfg(target_os = "linux")]
mod linux {
    use std::path::PathBuf;

    fn desktop_path(app_name: &str) -> PathBuf {
        let base = std::env::var("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                let h = std::env::var("HOME").unwrap_or_default();
                PathBuf::from(h).join(".config")
            });
        base.join("autostart")
            .join(format!("{app_name}.desktop"))
    }

    pub fn is_enabled(app_name: &str) -> bool {
        desktop_path(app_name).exists()
    }

    pub fn enable(app_name: &str, exe: &str) -> Result<(), String> {
        let path = desktop_path(app_name);
        if let Some(parent) = path.parent() { let _ = std::fs::create_dir_all(parent); }
        // Capitalise first letter for display name
        let display = {
            let mut c = app_name.chars();
            match c.next() {
                None => String::new(),
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
            }
        };
        let desktop = format!(
            "[Desktop Entry]\nType=Application\nName={display}\nExec={exe}\n\
             Hidden=false\nNoDisplay=false\nX-GNOME-Autostart-enabled=true\n"
        );
        std::fs::write(&path, desktop)
            .map_err(|e| format!("failed to write autostart .desktop: {e}"))
    }

    pub fn disable(app_name: &str) -> Result<(), String> {
        let path = desktop_path(app_name);
        if path.exists() {
            std::fs::remove_file(&path)
                .map_err(|e| format!("failed to remove autostart .desktop: {e}"))?;
        }
        Ok(())
    }
}

// ── Windows — registry HKCU Run key ──────────────────────────────────────────

#[cfg(target_os = "windows")]
mod windows {
    const REG_PATH: &str =
        r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";

    pub fn is_enabled(app_name: &str) -> bool {
        std::process::Command::new("reg")
            .args(["query", REG_PATH, "/v", app_name])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    pub fn enable(app_name: &str, exe: &str) -> Result<(), String> {
        let out = std::process::Command::new("reg")
            .args(["add", REG_PATH, "/v", app_name,
                   "/t", "REG_SZ", "/d", exe, "/f"])
            .output()
            .map_err(|e| format!("reg add failed: {e}"))?;
        if out.status.success() {
            Ok(())
        } else {
            Err(String::from_utf8_lossy(&out.stderr).into_owned())
        }
    }

    pub fn disable(app_name: &str) -> Result<(), String> {
        // Ignore "not found" errors — the key may never have been written.
        let _ = std::process::Command::new("reg")
            .args(["delete", REG_PATH, "/v", app_name, "/f"])
            .output();
        Ok(())
    }
}
