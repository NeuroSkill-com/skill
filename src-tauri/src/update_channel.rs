// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
//! Update channel preference (stable | rc).
//!
//! The Tauri updater plugin reads endpoints from `tauri.conf.json` at startup,
//! which can't easily be made user-configurable. Instead, every place that
//! checks for updates uses [`build_updater`] to construct an updater whose
//! endpoint reflects the user's saved channel choice.
//!
//! Storage is intentionally minimal: a single ASCII line in
//! `<app_local_data>/update-channel.txt`. Default (file missing or unreadable)
//! is `stable`. The preference lives in the Tauri app's local data because
//! it's purely a UI/updater concern — the daemon doesn't poll for updates.

use std::path::PathBuf;
use tauri::{AppHandle, Manager};

use crate::constants::APP_REPO_URL;

const PREF_FILE: &str = "update-channel.txt";

#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum UpdateChannel {
    #[default]
    Stable,
    Rc,
}

impl UpdateChannel {
    pub fn as_str(&self) -> &'static str {
        match self {
            UpdateChannel::Stable => "stable",
            UpdateChannel::Rc => "rc",
        }
    }

    /// Updater manifest URL for this channel.
    ///
    /// Stable resolves through GitHub's `releases/latest` semantics, which
    /// auto-skip pre-releases. RC points at the mutable `rc-latest` release
    /// the CI workflows keep in sync with the most recent build (RC or
    /// stable). RC users therefore receive stable releases too — once a
    /// stable release publishes, they overtake any in-flight RC naturally.
    pub fn endpoint_url(&self) -> String {
        match self {
            UpdateChannel::Stable => {
                format!("{APP_REPO_URL}/releases/latest/download/latest.json")
            }
            UpdateChannel::Rc => {
                format!("{APP_REPO_URL}/releases/download/rc-latest/latest.json")
            }
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "stable" => Some(UpdateChannel::Stable),
            "rc" => Some(UpdateChannel::Rc),
            _ => None,
        }
    }
}

fn pref_path(app: &AppHandle) -> Option<PathBuf> {
    app.path()
        .app_local_data_dir()
        .ok()
        .map(|d| d.join(PREF_FILE))
}

/// Read the saved channel preference. Errors are absorbed: a missing or
/// malformed file behaves the same as "no preference set" → stable.
pub fn read_channel(app: &AppHandle) -> UpdateChannel {
    let Some(path) = pref_path(app) else {
        return UpdateChannel::Stable;
    };
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| UpdateChannel::parse(&s))
        .unwrap_or_default()
}

/// Build a Tauri updater configured for the user's saved channel.
///
/// `tauri_plugin_updater::Builder` (the plugin-level builder) does *not*
/// expose an endpoints API — that lives only on the runtime
/// [`UpdaterBuilder`](tauri_plugin_updater::UpdaterBuilder) returned by
/// [`UpdaterExt::updater_builder`]. So channel awareness has to happen
/// per-check, not at plugin init time. Use this helper everywhere instead
/// of `app.updater()`.
pub fn build_updater(
    app: &AppHandle,
) -> Result<tauri_plugin_updater::Updater, Box<dyn std::error::Error + Send + Sync>> {
    use tauri_plugin_updater::UpdaterExt;
    let channel = read_channel(app);
    let url = url::Url::parse(&channel.endpoint_url())?;
    Ok(app.updater_builder().endpoints(vec![url])?.build()?)
}

// ── Tauri commands ────────────────────────────────────────────────────────────

#[tauri::command]
pub fn get_update_channel(app: AppHandle) -> String {
    read_channel(&app).as_str().to_string()
}

#[tauri::command]
pub fn set_update_channel(app: AppHandle, channel: String) -> Result<(), String> {
    let new = UpdateChannel::parse(&channel)
        .ok_or_else(|| format!("invalid channel: {channel:?} (expected 'stable' or 'rc')"))?;
    let path = pref_path(&app).ok_or_else(|| "app_local_data_dir unavailable".to_string())?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(&path, new.as_str()).map_err(|e| e.to_string())?;
    Ok(())
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateMeta {
    pub version: String,
    pub date: Option<String>,
    pub body: Option<String>,
}

/// Channel-aware replacement for `@tauri-apps/plugin-updater`'s `check()`.
/// Used by the frontend's "Check for updates" button so toggling the
/// channel takes effect immediately without an app restart.
#[tauri::command]
pub async fn channel_check_for_update(app: AppHandle) -> Result<Option<UpdateMeta>, String> {
    let updater = build_updater(&app).map_err(|e| e.to_string())?;
    let Some(update) = updater.check().await.map_err(|e| e.to_string())? else {
        return Ok(None);
    };
    Ok(Some(UpdateMeta {
        version: update.version.clone(),
        date: update.date.map(|d| d.to_string()),
        body: update.body.clone(),
    }))
}

/// Channel-aware replacement for the plugin's
/// `Update.downloadAndInstall(callback)`. Re-runs `check()` (so we can
/// recover from a stale frontend state) and streams progress as
/// `update-download-progress` events whose payload mirrors the plugin's
/// JS `DownloadEvent` shape so the frontend's existing switch on
/// `event.event` works unchanged.
#[tauri::command]
pub async fn channel_download_and_install(app: AppHandle) -> Result<(), String> {
    use tauri::Emitter;

    let updater = build_updater(&app).map_err(|e| e.to_string())?;
    let update = updater
        .check()
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "no update available on the selected channel".to_string())?;

    let app_progress = app.clone();
    let app_done = app.clone();
    let mut emitted_started = false;

    update
        .download_and_install(
            move |chunk, total| {
                if !emitted_started {
                    let _ = app_progress.emit(
                        "update-download-progress",
                        serde_json::json!({
                            "event": "Started",
                            "data": { "contentLength": total },
                        }),
                    );
                    emitted_started = true;
                }
                let _ = app_progress.emit(
                    "update-download-progress",
                    serde_json::json!({
                        "event": "Progress",
                        "data": { "chunkLength": chunk },
                    }),
                );
            },
            move || {
                let _ = app_done.emit(
                    "update-download-progress",
                    serde_json::json!({ "event": "Finished", "data": null }),
                );
            },
        )
        .await
        .map_err(|e| e.to_string())?;

    Ok(())
}
