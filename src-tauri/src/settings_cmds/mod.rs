// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
//! Device, filter, EEG model, app-settings, autostart, and update-interval Tauri commands.

pub mod activity_cmds;
pub mod device_cmds;
pub mod dnd_cmds;
pub mod location_cmds;
pub mod lsl_cmds;

// Re-export extracted commands so `use settings_cmds::X` keeps working in lib.rs.
pub use activity_cmds::{get_input_buckets, get_recent_active_windows, get_recent_input_activity};
pub use device_cmds::{get_device_capabilities, get_supported_companies};
pub use dnd_cmds::pick_ref_wav_file;
pub use location_cmds::test_location;

use crate::MutexExt;
use std::sync::Mutex;
use tauri::AppHandle;

use crate::autostart;
use crate::AppStateExt;
use crate::{constants::LOG_CONFIG_FILE, emit_status, mutate_and_save, AppState};
use skill_eeg::eeg_filter::PowerlineFreq;

// ── EEG filter commands ────────────────────────────────────────────────────────

#[tauri::command]
pub fn set_notch_preset(preset: Option<PowerlineFreq>, app: AppHandle) {
    if crate::daemon_cmds::set_notch_preset(preset).is_ok() {
        {
            let r = app.app_state();
            r.lock_or_recover().status.filter_config.notch = preset;
        }
        emit_status(&app);
    }
}

// ── Embedding overlap ─────────────────────────────────────────────────────────

// ── Logging config ────────────────────────────────────────────────────────────

#[tauri::command]
pub fn get_log_config(
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) -> crate::skill_log::LogConfig {
    state.lock_or_recover().logger.get_config()
}

#[tauri::command]
pub fn set_log_config(
    config: crate::skill_log::LogConfig,
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) {
    let s = state.lock_or_recover();
    let config_path = s.skill_dir.join(LOG_CONFIG_FILE);
    // Propagate TTS, LLM, and tool logging flags to their crate-level runtime atomics.
    crate::tts::set_logging(config.tts);
    crate::llm::set_llm_logging(config.llm || config.chat_store);
    crate::llm::set_tool_logging(config.tools);
    s.logger.set_config(config, &config_path);
}

// ── EEG model config ──────────────────────────────────────────────────────────

// ── EXG model catalog ─────────────────────────────────────────────────────────

// ── UMAP config ───────────────────────────────────────────────────────────────

// ── Theme & language ──────────────────────────────────────────────────────────

#[tauri::command]
pub fn get_theme_and_language(state: tauri::State<'_, Mutex<Box<AppState>>>) -> (String, String) {
    let s = state.lock_or_recover();
    (s.ui.theme.clone(), s.ui.language.clone())
}

#[tauri::command]
pub fn set_theme(theme: String, app: AppHandle, _state: tauri::State<'_, Mutex<Box<AppState>>>) {
    mutate_and_save(&app, |s| s.ui.theme = theme);
}

#[tauri::command]
pub fn set_language(
    language: String,
    app: AppHandle,
    _state: tauri::State<'_, Mutex<Box<AppState>>>,
) {
    mutate_and_save(&app, |s| s.ui.language = language);
}

#[tauri::command]
pub fn get_accent_color(_state: tauri::State<'_, Mutex<Box<AppState>>>) -> String {
    crate::daemon_cmds::fetch_accent_color().unwrap_or_default()
}

#[tauri::command]
pub fn set_accent_color(
    accent: String,
    app: AppHandle,
    _state: tauri::State<'_, Mutex<Box<AppState>>>,
) {
    if crate::daemon_cmds::set_accent_color(accent.clone()).is_ok() {
        app.app_state().lock_or_recover().ui.accent_color = accent;
    }
}

// ── Daily goal ────────────────────────────────────────────────────────────────

// Hooks CRUD + keyword suggestions — moved to hook_cmds.rs

#[tauri::command]
pub async fn open_session_for_timestamp(
    timestamp_utc: u64,
    app: AppHandle,
    _state: tauri::State<'_, Mutex<Box<AppState>>>,
) -> Result<(), String> {
    let Some(csv_path) = crate::daemon_cmds::find_history_session(timestamp_utc)
        .ok()
        .flatten()
    else {
        return Err("no session found for timestamp".to_owned());
    };
    crate::window_cmds::open_session_window(app, csv_path).await
}

// ── Autostart (launch at login) ────────────────────────────────────────────────

/// Returns `true` if the app is registered to launch at login.
///
/// Reads the OS-level registration directly (plist / .desktop / registry).
#[tauri::command]
pub fn get_autostart_enabled(app: AppHandle) -> bool {
    let name = app
        .config()
        .product_name
        .as_deref()
        .unwrap_or("skill")
        .to_lowercase();
    autostart::is_enabled(&name)
}

/// Enable or disable launch-at-login.
///
/// On macOS this writes / removes a LaunchAgent plist.
/// On Linux this writes / removes an XDG `.desktop` file.
/// On Windows this writes / deletes the `HKCU\...\Run` registry value.
#[tauri::command]
pub fn set_autostart_enabled(app: AppHandle, enabled: bool) -> Result<(), String> {
    let name = app
        .config()
        .product_name
        .as_deref()
        .unwrap_or("skill")
        .to_lowercase();
    autostart::set_enabled(&name, enabled).map_err(|e| e.to_string())
}

// ── Update-check interval ──────────────────────────────────────────────────────

/// Return the background update-check interval in seconds (0 = disabled).
#[tauri::command]
pub fn get_update_check_interval(_state: tauri::State<'_, Mutex<Box<AppState>>>) -> u64 {
    crate::daemon_cmds::fetch_update_check_interval().unwrap_or(0)
}

/// Persist a new update-check interval.
///
/// `secs` = 0 disables automatic checking.
/// The background task re-reads this value each cycle, so the change takes
/// effect without a restart.
#[tauri::command]
pub fn set_update_check_interval(
    secs: u64,
    _app: AppHandle,
    state: tauri::State<'_, Mutex<Box<AppState>>>,
) {
    if let Ok(persisted) = crate::daemon_cmds::set_update_check_interval(secs) {
        state.lock_or_recover().update_check_interval_secs = persisted;
    }
}

// ── Device config/status ──────────────────────────────────────────────────────

// ── NeuTTS configuration ───────────────────────────────────────────────────────

// ── File pickers ──────────────────────────────────────────────────────────────

/// Open a native file-picker dialog for selecting a GGUF model file.
///
/// Returns `None` if the user cancels.
#[tauri::command]
pub async fn pick_gguf_file() -> Option<String> {
    tokio::task::spawn_blocking(|| {
        rfd::FileDialog::new()
            .add_filter("GGUF model", &["gguf"])
            .set_title("Select GGUF model file")
            .pick_file()
            .map(|p| p.to_string_lossy().into_owned())
    })
    .await
    .ok()
    .flatten()
}

/// Open a native file-picker dialog for selecting EXG model weights.
///
/// Returns `None` if the user cancels.
#[tauri::command]
pub async fn pick_exg_weights_file() -> Option<String> {
    tokio::task::spawn_blocking(|| {
        rfd::FileDialog::new()
            .add_filter("Model weights", &["safetensors", "pth", "bin", "pt"])
            .set_title("Select EXG model weights")
            .pick_file()
            .map(|p| p.to_string_lossy().into_owned())
    })
    .await
    .ok()
    .flatten()
}

// ── Extension installation ────────────────────────────────────────────────────

/// Static metadata for one VS Code-based editor.
struct VsFork {
    /// Stable id sent over the Tauri boundary (e.g. "vscode", "cursor").
    id: &'static str,
    /// Human-readable display name.
    name: &'static str,
    /// Per-fork extensions directory under the user's home (e.g. ".vscode").
    ext_dir: &'static str,
    /// CLI binary names/paths to try, in order. Cross-platform: bare names
    /// resolve via PATH; absolute paths cover macOS/Windows installs where the
    /// CLI is bundled inside the app but not added to PATH by default.
    cli_candidates: &'static [&'static str],
}

/// All supported VS Code-family editors. To add a new fork, append here.
const VS_FORKS: &[VsFork] = &[
    VsFork {
        id: "vscode",
        name: "VS Code",
        ext_dir: ".vscode",
        cli_candidates: &[
            "code",
            "/Applications/Visual Studio Code.app/Contents/Resources/app/bin/code",
            "C:\\Program Files\\Microsoft VS Code\\bin\\code.cmd",
            "C:\\Users\\Public\\AppData\\Local\\Programs\\Microsoft VS Code\\bin\\code.cmd",
        ],
    },
    VsFork {
        id: "vscode-insiders",
        name: "VS Code Insiders",
        ext_dir: ".vscode-insiders",
        cli_candidates: &[
            "code-insiders",
            "/Applications/Visual Studio Code - Insiders.app/Contents/Resources/app/bin/code",
            "C:\\Program Files\\Microsoft VS Code Insiders\\bin\\code-insiders.cmd",
        ],
    },
    VsFork {
        id: "vscodium",
        name: "VSCodium",
        ext_dir: ".vscode-oss",
        cli_candidates: &[
            "codium",
            "/Applications/VSCodium.app/Contents/Resources/app/bin/codium",
            "C:\\Program Files\\VSCodium\\bin\\codium.cmd",
        ],
    },
    VsFork {
        id: "cursor",
        name: "Cursor",
        ext_dir: ".cursor",
        cli_candidates: &[
            "cursor",
            "/Applications/Cursor.app/Contents/Resources/app/bin/cursor",
            "C:\\Users\\Public\\AppData\\Local\\Programs\\cursor\\resources\\app\\bin\\cursor.cmd",
        ],
    },
    VsFork {
        id: "windsurf",
        name: "Windsurf",
        ext_dir: ".windsurf",
        cli_candidates: &[
            "windsurf",
            "/Applications/Windsurf.app/Contents/Resources/app/bin/windsurf",
            "C:\\Program Files\\Windsurf\\bin\\windsurf.cmd",
        ],
    },
    VsFork {
        id: "trae",
        name: "Trae",
        ext_dir: ".trae",
        cli_candidates: &[
            "trae",
            "/Applications/Trae.app/Contents/Resources/app/bin/trae",
        ],
    },
    VsFork {
        id: "positron",
        name: "Positron",
        ext_dir: ".positron",
        cli_candidates: &[
            "positron",
            "/Applications/Positron.app/Contents/Resources/app/bin/positron",
        ],
    },
];

impl VsFork {
    fn ext_dir_path(&self) -> Option<std::path::PathBuf> {
        dirs::home_dir().map(|h| h.join(self.ext_dir).join("extensions"))
    }

    /// True when the editor itself appears installed (extensions dir present
    /// or any candidate CLI runs successfully).
    fn available(&self) -> bool {
        if let Some(d) = self.ext_dir_path() {
            if d.is_dir() {
                return true;
            }
        }
        self.find_cli().is_some()
    }

    /// True when the NeuroSkill extension is installed under this fork.
    fn extension_installed(&self, extension_id: &str) -> bool {
        let Some(dir) = self.ext_dir_path() else {
            return false;
        };
        let prefix = format!("{extension_id}-");
        let Ok(entries) = std::fs::read_dir(&dir) else {
            return false;
        };
        for entry in entries.flatten() {
            if let Some(name) = entry.file_name().to_str() {
                if name == extension_id || name.starts_with(&prefix) {
                    return true;
                }
            }
        }
        false
    }

    /// Locate a working CLI binary for this fork. Cross-platform: tries each
    /// candidate (bare name → PATH lookup; absolute → file-system check).
    fn find_cli(&self) -> Option<String> {
        for p in self.cli_candidates {
            if std::process::Command::new(p)
                .arg("--version")
                .output()
                .is_ok()
            {
                return Some((*p).to_string());
            }
        }
        None
    }
}

fn find_fork(id: &str) -> Option<&'static VsFork> {
    VS_FORKS.iter().find(|f| f.id == id)
}

#[tauri::command]
pub async fn install_extension(extension_id: String) -> Result<serde_json::Value, String> {
    tokio::task::spawn_blocking(move || {
        let result = if let Some(fork) = find_fork(&extension_id) {
            install_vscode_extension(fork)
        } else {
            match extension_id.as_str() {
                // Edge uses the same Chrome MV3 build (Edge is Chromium-based)
                "chrome" | "edge" | "firefox" | "safari" => {
                    install_browser_extension(&extension_id)
                }
                _ => Err(format!("Unknown extension: {extension_id}")),
            }
        };
        match result {
            Ok(msg) => Ok(serde_json::json!({"ok": true, "message": msg})),
            Err(e) => Ok(serde_json::json!({"ok": false, "message": e})),
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

fn install_vscode_extension(fork: &VsFork) -> Result<String, String> {
    let code_bin = fork.find_cli().ok_or_else(|| {
        format!(
            "{} CLI not found in PATH or known install locations",
            fork.name
        )
    })?;

    let ext_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("extensions")
        .join("vscode");
    if !ext_dir.join("package.json").exists() {
        return Err(format!("Extension not found at {}", ext_dir.display()));
    }

    // Build
    std::process::Command::new("npm")
        .arg("install")
        .current_dir(&ext_dir)
        .output()
        .map_err(|e| format!("npm install: {e}"))?;
    std::process::Command::new("npx")
        .args(["tsc", "-p", "tsconfig.json"])
        .current_dir(&ext_dir)
        .output()
        .map_err(|e| format!("tsc: {e}"))?;
    let vsce = std::process::Command::new("npx")
        .args(["@vscode/vsce", "package", "--no-dependencies"])
        .current_dir(&ext_dir)
        .output()
        .map_err(|e| format!("vsce: {e}"))?;
    let output = String::from_utf8_lossy(&vsce.stdout);
    let vsix = output
        .lines()
        .find_map(|l| {
            l.strip_prefix("Packaged: ")
                .map(|p| ext_dir.join(p.trim()).to_string_lossy().to_string())
        })
        .unwrap_or_else(|| {
            ext_dir
                .join("neuroskill-0.1.0.vsix")
                .to_string_lossy()
                .to_string()
        });

    // Install
    let install = std::process::Command::new(&code_bin)
        .args(["--install-extension", &vsix, "--force"])
        .output()
        .map_err(|e| format!("install: {e}"))?;
    if install.status.success() {
        Ok(format!(
            "Extension installed in {}. Reload it to activate.",
            fork.name
        ))
    } else {
        Err(String::from_utf8_lossy(&install.stderr).to_string())
    }
}

fn install_browser_extension(target: &str) -> Result<String, String> {
    let ext_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("extensions")
        .join("browser");
    if !ext_dir.join("package.json").exists() {
        return Err(format!(
            "Browser extension not found at {}. Run: git submodule update --init",
            ext_dir.display()
        ));
    }

    // Edge uses the same Chromium build as Chrome (same MV3 manifest)
    let build_target = if target == "edge" { "chrome" } else { target };

    // npm install — pipe output for diagnostics
    let npm_out = std::process::Command::new("npm")
        .arg("install")
        .current_dir(&ext_dir)
        .output()
        .map_err(|e| {
            format!("npm install failed to start: {e}. Make sure Node.js is installed.")
        })?;
    if !npm_out.status.success() {
        return Err(format!(
            "npm install failed: {}",
            String::from_utf8_lossy(&npm_out.stderr)
        ));
    }

    let build = std::process::Command::new("node")
        .args(["build/build.mjs"])
        .env("BROWSER_TARGET", build_target)
        .current_dir(&ext_dir)
        .output()
        .map_err(|e| format!("build: {e}"))?;
    if !build.status.success() {
        return Err(format!(
            "Build failed: {}",
            String::from_utf8_lossy(&build.stderr)
        ));
    }

    let dist = ext_dir.join("dist").join(build_target);

    // Safari is special — needs Xcode wrapping
    if target == "safari" {
        return install_safari_wrapper(&dist);
    }

    let load_hint = match target {
        "chrome" => "chrome://extensions → Developer mode → Load unpacked",
        "edge" => "edge://extensions → Developer mode → Load unpacked",
        "firefox" => "about:debugging → This Firefox → Load Temporary Add-on",
        _ => "Load it in your browser's extension settings",
    };
    Ok(format!(
        "Extension built at {}. {}",
        dist.display(),
        load_hint,
    ))
}

/// Wrap the Safari MV3 build into a Safari Web Extension App via
/// `xcrun safari-web-extension-converter`, then build with `xcodebuild`,
/// then open the resulting .app so Safari registers the extension.
///
/// Three things must happen for a Safari extension to work:
///   1. Wrapping (creates Xcode project)
///   2. Building (xcodebuild)
///   3. First-launch (the user opens the .app once so Safari sees it)
///
/// macOS-only.
fn install_safari_wrapper(dist: &std::path::Path) -> Result<String, String> {
    if cfg!(not(target_os = "macos")) {
        return Err("Safari extensions only work on macOS".into());
    }

    // 1. Verify xcrun safari-web-extension-converter is available
    let conv_check = std::process::Command::new("xcrun")
        .args(["--find", "safari-web-extension-converter"])
        .output()
        .map_err(|e| format!("xcrun not found: {e}. Install Xcode Command Line Tools."))?;
    if !conv_check.status.success() {
        return Err(
            "safari-web-extension-converter not available. Run: xcode-select --install".into(),
        );
    }

    // 2. Pick a stable output directory under the user's home
    let home = std::env::var("HOME").map_err(|_| "HOME not set".to_string())?;
    let wrapper_root = std::path::PathBuf::from(home)
        .join("Library")
        .join("Application Support")
        .join("skill")
        .join("safari-extension");
    // Wipe any prior generation so the converter's output is clean
    if wrapper_root.exists() {
        let _ = std::fs::remove_dir_all(&wrapper_root);
    }
    std::fs::create_dir_all(&wrapper_root).map_err(|e| format!("create wrapper dir: {e}"))?;

    // 3. Run the converter — non-interactive, no prompt, no Xcode open
    let conv = std::process::Command::new("xcrun")
        .args([
            "safari-web-extension-converter",
            dist.to_string_lossy().as_ref(),
            "--project-location",
            wrapper_root.to_string_lossy().as_ref(),
            "--app-name",
            "NeuroSkill Browser",
            "--bundle-identifier",
            "com.neuroskill.browser-extension",
            "--no-prompt",
            "--no-open",
            "--macos-only",
            "--copy-resources",
        ])
        .output()
        .map_err(|e| format!("converter failed to start: {e}"))?;
    if !conv.status.success() {
        return Err(format!(
            "safari-web-extension-converter failed: {}",
            String::from_utf8_lossy(&conv.stderr)
        ));
    }

    // 4. Locate the generated .xcodeproj
    let project_dir = wrapper_root.join("NeuroSkill Browser");
    let xcodeproj = project_dir.join("NeuroSkill Browser.xcodeproj");
    if !xcodeproj.exists() {
        return Err(format!(
            "expected Xcode project at {} but it doesn't exist",
            xcodeproj.display()
        ));
    }

    // 4a. Patch the project.pbxproj — the converter assigns mismatched bundle
    // IDs that fail xcodebuild's ValidateEmbeddedBinary check. Make both
    // targets share the same prefix.
    let pbxproj = xcodeproj.join("project.pbxproj");
    if let Ok(content) = std::fs::read_to_string(&pbxproj) {
        let patched = content
            .replace(
                "PRODUCT_BUNDLE_IDENTIFIER = \"com.neuroskill.NeuroSkill-Browser\";",
                "PRODUCT_BUNDLE_IDENTIFIER = \"com.neuroskill.browser\";",
            )
            .replace(
                "PRODUCT_BUNDLE_IDENTIFIER = \"com.neuroskill.browser-extension.Extension\";",
                "PRODUCT_BUNDLE_IDENTIFIER = \"com.neuroskill.browser.Extension\";",
            );
        let _ = std::fs::write(&pbxproj, patched);
    }

    // 5. Build with xcodebuild
    let build_dir = project_dir.join("build");
    std::fs::create_dir_all(&build_dir).ok();
    // Pick a signing identity:
    //   1. APPLE_SIGNING_IDENTITY env var (CI / release builds)
    //   2. First "Developer ID Application" cert in keychain (Safari-trusted)
    //   3. Fall back to ad-hoc (extension loads but Safari requires "Allow Unsigned")
    //
    // Self-signed certs (no Apple Team ID) and "Apple Development" certs are
    // intentionally NOT auto-selected — Safari treats them the same as unsigned
    // for extension trust purposes. Better to ad-hoc sign and tell the user
    // honestly than to falsely promise it'll work.
    let sign_id = std::env::var("APPLE_SIGNING_IDENTITY")
        .ok()
        .unwrap_or_else(|| {
            let out = std::process::Command::new("security")
                .args(["find-identity", "-v", "-p", "codesigning"])
                .output();
            if let Ok(out) = out {
                let stdout = String::from_utf8_lossy(&out.stdout);
                for line in stdout.lines() {
                    if line.contains("Developer ID Application") {
                        if let Some(name) = line.split('"').nth(1) {
                            return name.to_string();
                        }
                    }
                }
            }
            "-".to_string() // ad-hoc
        });
    let signing_required = sign_id != "-";
    let safari_will_trust = sign_id.starts_with("Developer ID Application");

    let mut xcodebuild_args = vec![
        "-project".to_string(),
        xcodeproj.to_string_lossy().to_string(),
        "-scheme".to_string(),
        "NeuroSkill Browser".to_string(),
        "-configuration".to_string(),
        "Release".to_string(),
        "-derivedDataPath".to_string(),
        build_dir.to_string_lossy().to_string(),
        format!("CODE_SIGN_IDENTITY={sign_id}"),
        format!(
            "CODE_SIGNING_REQUIRED={}",
            if signing_required { "YES" } else { "NO" }
        ),
        format!(
            "CODE_SIGNING_ALLOWED={}",
            if signing_required { "YES" } else { "NO" }
        ),
        "build".to_string(),
    ];
    // Disable Xcode automatic signing (so it uses our identity, not a provisioning profile)
    xcodebuild_args.push("CODE_SIGN_STYLE=Manual".to_string());

    let build = std::process::Command::new("xcodebuild")
        .args(&xcodebuild_args)
        .output()
        .map_err(|e| format!("xcodebuild failed to start: {e}"))?;
    if !build.status.success() {
        let stderr = String::from_utf8_lossy(&build.stderr);
        let stdout = String::from_utf8_lossy(&build.stdout);
        return Err(format!(
            "xcodebuild failed:\n{}\n{}",
            stdout.lines().rev().take(20).collect::<Vec<_>>().join("\n"),
            stderr.lines().rev().take(10).collect::<Vec<_>>().join("\n")
        ));
    }

    // 6. Find the resulting .app
    let built_app = std::path::PathBuf::from(&build_dir)
        .join("Build")
        .join("Products")
        .join("Release")
        .join("NeuroSkill Browser.app");
    if !built_app.exists() {
        return Err(format!(
            "expected .app at {} but it wasn't produced",
            built_app.display()
        ));
    }

    // 7. Copy to ~/Applications/. Launch Services doesn't reliably index
    //    apps under Library/Application Support, so Safari's plugin scanner
    //    doesn't find the .appex there.
    let user_apps =
        std::path::PathBuf::from(std::env::var("HOME").unwrap_or_default()).join("Applications");
    std::fs::create_dir_all(&user_apps).ok();
    let final_app = user_apps.join("NeuroSkill Browser.app");
    if final_app.exists() {
        let _ = std::fs::remove_dir_all(&final_app);
    }
    let cp = std::process::Command::new("cp")
        .args([
            "-R",
            &built_app.to_string_lossy(),
            &final_app.to_string_lossy(),
        ])
        .output()
        .map_err(|e| format!("cp failed: {e}"))?;
    if !cp.status.success() {
        return Err(format!(
            "cp to ~/Applications failed: {}",
            String::from_utf8_lossy(&cp.stderr)
        ));
    }

    // 7a. Re-sign the whole bundle with --deep so both the app and the .appex
    //     get signed by the same identity. Without this, Safari may refuse
    //     the embedded extension even if xcodebuild signed the app.
    let codesign = std::process::Command::new("codesign")
        .args([
            "--force",
            "--deep",
            "--sign",
            &sign_id,
            "--options",
            "runtime",
            &final_app.to_string_lossy(),
        ])
        .output()
        .map_err(|e| format!("codesign failed to start: {e}"))?;
    if !codesign.status.success() {
        // Don't fail — the build is still useful even if re-signing fails
        eprintln!(
            "codesign --deep warning: {}",
            String::from_utf8_lossy(&codesign.stderr)
        );
    }

    // 8. Re-register with Launch Services so Safari sees the .appex
    let _ = std::process::Command::new(
        "/System/Library/Frameworks/CoreServices.framework/Versions/Current/Frameworks/LaunchServices.framework/Versions/Current/Support/lsregister",
    )
    .args(["-f", "-R", "-trusted", &final_app.to_string_lossy()])
    .output();

    // 9. Try to enable Safari's developer prefs so the user has fewer manual steps
    //    (these may not always work due to SIP / TCC, but worth trying)
    let _ = std::process::Command::new("defaults")
        .args([
            "write",
            "com.apple.Safari",
            "IncludeDevelopMenu",
            "-bool",
            "YES",
        ])
        .output();

    // 10. Open the app once so Safari can register the extension on first launch
    let _ = std::process::Command::new("open").arg(&final_app).spawn();

    // 11. After a brief pause, open Safari's Extensions preferences so the user
    //     lands directly on the toggle. Best-effort — failure is harmless.
    let _ = std::process::Command::new("sh")
        .arg("-c")
        .arg("(sleep 1 && open 'x-apple.systempreferences:com.apple.preference.safari?Extensions' 2>/dev/null || open -a Safari 'x-apple.systempreferences:com.apple.preference.safari?Extensions') &")
        .spawn();

    // Verify Gatekeeper actually accepts the result. Safari refuses to load
    // extensions from rejected apps, so this is the ground-truth check.
    let gatekeeper_ok = std::process::Command::new("spctl")
        .args([
            "--assess",
            "--type",
            "execute",
            &final_app.to_string_lossy(),
        ])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    let msg = if safari_will_trust && gatekeeper_ok {
        format!(
            "Installed at {} (signed with {}).\n\n\
             Safari trusts this signature. To enable:\n\
             Safari → Settings → Extensions → check 'NeuroSkill Browser Extension'.",
            final_app.display(),
            sign_id,
        )
    } else {
        let sig_note = if !signing_required {
            "ad-hoc signed (no certificate)".to_string()
        } else if !gatekeeper_ok {
            format!("signed with {sign_id} but Gatekeeper rejected it")
        } else {
            format!("signed with {sign_id} (not a Developer ID Application cert)")
        };
        format!(
            "Installed at {} ({}).\n\n\
             Safari only loads extensions from apps Gatekeeper trusts, which \
             requires a 'Developer ID Application' certificate. Set \
             APPLE_SIGNING_IDENTITY to a 'Developer ID Application: …' identity \
             on the build machine and reinstall — or click 'Allow Unsigned' to \
             enable it on this Mac for development only.",
            final_app.display(),
            sig_note,
        )
    };
    Ok(msg)
}

#[tauri::command]
pub async fn check_extensions_installed() -> Result<serde_json::Value, String> {
    tokio::task::spawn_blocking(|| {
        let mut result = serde_json::Map::new();

        // One entry per VS Code-family fork the user has installed. Each entry
        // is `{name, available, installed}` so the UI can list every fork
        // independently and show a per-fork install button.
        let mut forks = Vec::new();
        for fork in VS_FORKS {
            let available = fork.available();
            let installed = fork.extension_installed("neuroskill.neuroskill");
            forks.push(serde_json::json!({
                "id": fork.id,
                "name": fork.name,
                "available": available,
                "installed": installed,
            }));
        }
        result.insert("vscode_forks".into(), serde_json::Value::Array(forks));

        // Legacy single boolean for older UIs / clients (true if the extension
        // is installed in any fork).
        let any_vscode = VS_FORKS
            .iter()
            .any(|f| f.extension_installed("neuroskill.neuroskill"));
        result.insert("vscode".into(), any_vscode.into());

        // Browser extensions: check if dist directory exists with a manifest
        let ext_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("extensions")
            .join("browser")
            .join("dist");
        let chrome_built = ext_dir.join("chrome").join("manifest.json").exists();
        result.insert("chrome".into(), chrome_built.into());
        // Edge uses the same Chromium build as Chrome
        result.insert("edge".into(), chrome_built.into());
        result.insert(
            "firefox".into(),
            ext_dir
                .join("firefox")
                .join("manifest.json")
                .exists()
                .into(),
        );
        result.insert(
            "safari".into(),
            ext_dir.join("safari").join("manifest.json").exists().into(),
        );

        Ok(serde_json::Value::Object(result))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Enable Safari's Develop menu and toggle "Allow Unsigned Extensions".
///
/// What this does:
///   1. Sets `IncludeDevelopMenu = true` in Safari prefs (persistent)
///   2. Opens Safari (so the menu becomes available)
///   3. Uses AppleScript via osascript to click "Allow Unsigned Extensions"
///      (requires Accessibility permission for the parent app)
///
/// "Allow Unsigned Extensions" is intentionally a per-session toggle that
/// resets every Safari restart — Apple doesn't expose a persistent pref.
/// So this command must be re-run after each Safari relaunch.
#[tauri::command]
pub async fn enable_safari_unsigned_extensions() -> Result<serde_json::Value, String> {
    if cfg!(not(target_os = "macos")) {
        return Ok(serde_json::json!({"ok": false, "message": "macOS only"}));
    }

    tokio::task::spawn_blocking(|| -> Result<serde_json::Value, String> {
        // 1. Persist "Show Develop menu in menu bar"
        let _ = std::process::Command::new("defaults")
            .args(["write", "com.apple.Safari", "IncludeDevelopMenu", "-bool", "YES"])
            .output();

        // Also enable the "Show features for web developers" pref so the
        // Develop menu actually appears in modern Safari (16+).
        let _ = std::process::Command::new("defaults")
            .args([
                "write",
                "com.apple.Safari",
                "WebKitDeveloperExtras",
                "-bool",
                "YES",
            ])
            .output();

        // 2. Quit Safari if running so it picks up the new IncludeDevelopMenu
        //    pref. Safari only reads developer prefs on launch.
        let _ = std::process::Command::new("osascript")
            .args(["-e", r#"tell application "Safari" to quit"#])
            .output();
        std::thread::sleep(std::time::Duration::from_millis(1200));

        // 3. Open Safari fresh — the Develop menu will now be available
        let _ = std::process::Command::new("open").args(["-a", "Safari"]).spawn();
        std::thread::sleep(std::time::Duration::from_millis(2500));

        // 3. Try to click "Allow Unsigned Extensions" via AppleScript.
        //    This requires Accessibility permission for the calling app.
        //    If it fails (no permission), we return a message instructing
        //    the user to do it manually — we've at least made the menu visible.
        let script = r#"
tell application "System Events"
    if not (exists process "Safari") then return "safari_not_running"
    tell process "Safari"
        try
            set developMenu to menu "Develop" of menu bar 1
        on error
            return "develop_menu_missing"
        end try
        try
            set theItem to first menu item of developMenu whose name contains "Allow unsigned"
            click theItem
            return "clicked"
        on error errMsg
            return "menu_item_not_found: " & errMsg
        end try
    end tell
end tell
        "#;
        let result = std::process::Command::new("osascript")
            .args(["-e", script])
            .output();

        match result {
            Ok(out) if out.status.success() => {
                let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if stdout == "clicked" {
                    Ok(serde_json::json!({
                        "ok": true,
                        "message": "Allow Unsigned Extensions enabled. Safari will reset this on next restart.",
                        "auto_clicked": true,
                    }))
                } else {
                    Ok(serde_json::json!({
                        "ok": true,
                        "message": format!(
                            "Develop menu is now visible. AppleScript couldn't auto-click ({stdout}). \
                             Manually: Safari → Develop → Allow Unsigned Extensions."
                        ),
                        "auto_clicked": false,
                    }))
                }
            }
            Ok(out) => {
                let stderr = String::from_utf8_lossy(&out.stderr);
                // Most common failure: "osascript is not allowed assistive access"
                let needs_accessibility = stderr.contains("not allowed")
                    || stderr.contains("(-1743)")
                    || stderr.contains("assistive access");
                if needs_accessibility {
                    // Open the Accessibility settings pane so user can grant permission
                    let _ = std::process::Command::new("open")
                        .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")
                        .spawn();
                    Ok(serde_json::json!({
                        "ok": false,
                        "message": "Accessibility permission required. Grant access in System Settings → Privacy → Accessibility, then try again. (Develop menu has been enabled — you can also toggle manually: Safari → Develop → Allow Unsigned Extensions.)",
                        "needs_accessibility": true,
                    }))
                } else {
                    Ok(serde_json::json!({
                        "ok": false,
                        "message": format!("AppleScript failed: {stderr}. Develop menu has been enabled — toggle manually."),
                    }))
                }
            }
            Err(e) => Err(format!("osascript not found: {e}")),
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

// ── Re-embed all raw EXG data ─────────────────────────────────────────────────
