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

#[tauri::command]
pub async fn install_extension(extension_id: String) -> Result<serde_json::Value, String> {
    tokio::task::spawn_blocking(move || {
        let result = match extension_id.as_str() {
            "vscode" => install_vscode_extension(),
            "chrome" | "firefox" | "safari" => install_browser_extension(&extension_id),
            _ => Err(format!("Unknown extension: {extension_id}")),
        };
        match result {
            Ok(msg) => Ok(serde_json::json!({"ok": true, "message": msg})),
            Err(e) => Ok(serde_json::json!({"ok": false, "message": e})),
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

fn install_vscode_extension() -> Result<String, String> {
    let code_paths = [
        "code",
        "codium",
        "cursor",
        "/Applications/Visual Studio Code.app/Contents/Resources/app/bin/code",
        "/Applications/VSCodium.app/Contents/Resources/app/bin/codium",
        "/Applications/Cursor.app/Contents/Resources/app/bin/code",
    ];
    let mut code_bin = None;
    for p in &code_paths {
        if std::process::Command::new(p)
            .arg("--version")
            .output()
            .is_ok()
        {
            code_bin = Some(p.to_string());
            break;
        }
    }
    let code_bin = code_bin.ok_or("VS Code / VSCodium / Cursor not found")?;

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
        Ok("VS Code extension installed. Reload VS Code to activate.".into())
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
            "Browser extension not found at {}",
            ext_dir.display()
        ));
    }

    std::process::Command::new("npm")
        .arg("install")
        .current_dir(&ext_dir)
        .output()
        .map_err(|e| format!("npm install: {e}"))?;

    let env_target = format!("BROWSER_TARGET={target}");
    let build = std::process::Command::new("node")
        .args(["build/build.mjs"])
        .env("BROWSER_TARGET", target)
        .current_dir(&ext_dir)
        .output()
        .map_err(|e| format!("build: {e}"))?;
    if !build.status.success() {
        return Err(format!(
            "Build failed: {}",
            String::from_utf8_lossy(&build.stderr)
        ));
    }

    let dist = ext_dir.join("dist").join(target);
    Ok(format!(
        "Extension built at {}. Load it in your browser's extension settings.",
        dist.display()
    ))
}

#[tauri::command]
pub async fn check_extensions_installed() -> Result<serde_json::Value, String> {
    tokio::task::spawn_blocking(|| {
        let mut result = serde_json::Map::new();

        // VS Code: check if neuroskill extension is listed
        let vscode = std::process::Command::new("code")
            .args(["--list-extensions"])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).contains("neuroskill"))
            .unwrap_or(false);
        result.insert("vscode".into(), vscode.into());

        // Browser extensions: check if dist directory exists with a manifest
        let ext_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("extensions")
            .join("browser")
            .join("dist");
        result.insert(
            "chrome".into(),
            ext_dir.join("chrome").join("manifest.json").exists().into(),
        );
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

// ── Re-embed all raw EXG data ─────────────────────────────────────────────────
