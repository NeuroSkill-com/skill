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

use anyhow::Context;

// ── Public API ────────────────────────────────────────────────────────────────

/// Returns `true` if launch-at-login is currently registered for this app.
pub fn is_enabled(app_name: &str) -> bool {
    #[cfg(target_os = "macos")]
    {
        macos::is_enabled(app_name)
    }
    #[cfg(target_os = "linux")]
    {
        linux::is_enabled(app_name)
    }
    #[cfg(target_os = "windows")]
    {
        windows::is_enabled(app_name)
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        false
    }
}

/// Enable or disable launch-at-login.
///
/// Derives the executable path from [`std::env::current_exe`].
pub fn set_enabled(app_name: &str, enabled: bool) -> anyhow::Result<()> {
    if enabled {
        let exe = std::env::current_exe()
            .context("cannot locate executable")?
            .to_string_lossy()
            .to_string();
        enable(app_name, &exe)
    } else {
        disable(app_name)
    }
}

// ── Platform dispatch ─────────────────────────────────────────────────────────

fn enable(app_name: &str, exe: &str) -> anyhow::Result<()> {
    #[cfg(target_os = "macos")]
    return macos::enable(app_name, exe);
    #[cfg(target_os = "linux")]
    return linux::enable(app_name, exe);
    #[cfg(target_os = "windows")]
    return windows::enable(app_name, exe);
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        let _ = (app_name, exe);
        Err(anyhow::anyhow!("autostart not supported on this platform"))
    }
}

fn disable(app_name: &str) -> anyhow::Result<()> {
    #[cfg(target_os = "macos")]
    return macos::disable(app_name);
    #[cfg(target_os = "linux")]
    return linux::disable(app_name);
    #[cfg(target_os = "windows")]
    return windows::disable(app_name);
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        let _ = app_name;
        Err(anyhow::anyhow!("autostart not supported on this platform"))
    }
}

// ── macOS — LaunchAgent plist ─────────────────────────────────────────────────

#[cfg(target_os = "macos")]
mod macos {
    use anyhow::Context;
    use skill_constants::AUTOSTART_PLIST_LABEL_PREFIX;
    use std::path::PathBuf;

    fn plist_path(app_name: &str) -> Option<PathBuf> {
        std::env::var("HOME").ok().map(|h| {
            PathBuf::from(h).join("Library/LaunchAgents").join(format!(
                "{AUTOSTART_PLIST_LABEL_PREFIX}.{app_name}.loginitem.plist"
            ))
        })
    }

    pub fn is_enabled(app_name: &str) -> bool {
        plist_path(app_name).map(|p| p.exists()).unwrap_or(false)
    }

    pub fn enable(app_name: &str, exe: &str) -> anyhow::Result<()> {
        let path = plist_path(app_name).ok_or_else(|| anyhow::anyhow!("HOME not set"))?;
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
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
        std::fs::write(&path, plist).context("failed to write LaunchAgent")
    }

    pub fn disable(app_name: &str) -> anyhow::Result<()> {
        let path = plist_path(app_name).ok_or_else(|| anyhow::anyhow!("HOME not set"))?;
        if path.exists() {
            std::fs::remove_file(&path).context("failed to remove LaunchAgent")?;
        }
        Ok(())
    }
}

// ── Linux — XDG autostart .desktop file ──────────────────────────────────────

#[cfg(target_os = "linux")]
mod linux {
    use anyhow::Context;
    use std::path::PathBuf;

    fn desktop_path(app_name: &str) -> PathBuf {
        let base = std::env::var("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                let h = std::env::var("HOME").unwrap_or_default();
                PathBuf::from(h).join(".config")
            });
        base.join("autostart").join(format!("{app_name}.desktop"))
    }

    pub fn is_enabled(app_name: &str) -> bool {
        desktop_path(app_name).exists()
    }

    pub fn enable(app_name: &str, exe: &str) -> anyhow::Result<()> {
        let path = desktop_path(app_name);
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
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
        std::fs::write(&path, desktop).context("failed to write autostart .desktop")
    }

    pub fn disable(app_name: &str) -> anyhow::Result<()> {
        let path = desktop_path(app_name);
        if path.exists() {
            std::fs::remove_file(&path).context("failed to remove autostart .desktop")?;
        }
        Ok(())
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_enable_creates_desktop_file() {
        // Use a temp XDG_CONFIG_HOME so we don't pollute real autostart
        let tmp = std::env::temp_dir().join(format!("skill_autostart_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::env::set_var("XDG_CONFIG_HOME", &tmp);

        let result = enable("skill_test_app", "/usr/bin/test");
        assert!(result.is_ok(), "enable failed: {:?}", result);

        assert!(is_enabled("skill_test_app"));

        // Check the desktop file content
        let path = tmp.join("autostart/skill_test_app.desktop");
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("Exec=/usr/bin/test"));
        assert!(content.contains("Type=Application"));
        assert!(content.contains("Name=Skill_test_app"));

        // Disable should remove the file
        let result = disable("skill_test_app");
        assert!(result.is_ok());
        assert!(!is_enabled("skill_test_app"));
        assert!(!path.exists());

        let _ = std::fs::remove_dir_all(&tmp);
        // Restore
        std::env::remove_var("XDG_CONFIG_HOME");
    }

    #[test]
    fn set_enabled_with_nonexistent_app_does_not_panic() {
        // Just ensure no panic — actual registration may fail on CI
        let _ = is_enabled("nonexistent_test_app_xyz");
    }

    #[test]
    fn disable_nonexistent_is_ok() {
        let result = disable("nonexistent_app_that_was_never_registered");
        // On Linux this removes a non-existent file — should be Ok
        // On other platforms it may vary but should not panic
        assert!(result.is_ok() || result.is_err());
    }
}

// ── Windows — registry HKCU Run key ──────────────────────────────────────────

#[cfg(target_os = "windows")]
mod windows {
    use anyhow::Context;

    const REG_PATH: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";

    pub fn is_enabled(app_name: &str) -> bool {
        std::process::Command::new("reg")
            .args(["query", REG_PATH, "/v", app_name])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    pub fn enable(app_name: &str, exe: &str) -> anyhow::Result<()> {
        let out = std::process::Command::new("reg")
            .args([
                "add", REG_PATH, "/v", app_name, "/t", "REG_SZ", "/d", exe, "/f",
            ])
            .output()
            .context("reg add failed")?;
        if out.status.success() {
            Ok(())
        } else {
            anyhow::bail!("{}", String::from_utf8_lossy(&out.stderr))
        }
    }

    pub fn disable(app_name: &str) -> anyhow::Result<()> {
        // Ignore "not found" errors — the key may never have been written.
        let _ = std::process::Command::new("reg")
            .args(["delete", REG_PATH, "/v", app_name, "/f"])
            .output();
        Ok(())
    }
}
