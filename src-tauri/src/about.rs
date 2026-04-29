// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
//! About window data.
//!
//! # What lives here
//!
//! * [`AboutInfo`] — serialisable struct returned to the frontend.
//! * [`get_about_info`] — Tauri command that the `/about` page invokes.
//! * [`open_about_window`] — Tauri command that opens the custom About window.

use base64::{engine::general_purpose::STANDARD, Engine as _};
use tauri::AppHandle;

use crate::constants::{
    APP_ACKNOWLEDGEMENTS, APP_AUTHORS, APP_COPYRIGHT, APP_DISCORD_URL, APP_DISPLAY_NAME,
    APP_LICENSE, APP_LICENSE_NAME, APP_LICENSE_URL, APP_REPO_URL, APP_TAGLINE, APP_WEBSITE,
    APP_WEBSITE_LABEL,
};

// ── Serialisable about payload ────────────────────────────────────────────────

/// All about-page data in one serialisable blob.
/// The frontend receives this via [`get_about_info`].
#[derive(serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AboutInfo {
    pub name: String,
    /// Raw version from the bundle's CFBundleShortVersionString (e.g.
    /// `0.5.1-rc.3` for an RC build that was promoted to stable).
    pub version: String,
    /// User-facing version with any RC suffix stripped (e.g. `0.5.1`).
    /// The "is this an RC?" signal belongs in [`channel`], not the version.
    pub display_version: String,
    /// User's selected update channel: `"stable"` or `"rc"`. Drives whether
    /// the frontend appends a `(RC · {commit})` marker.
    pub channel: String,
    /// First seven characters of the build's git commit, baked in by
    /// `build.rs`. Empty when the build wasn't done from a git checkout.
    pub commit_short: String,
    /// Full `git describe --tags --always --dirty` output for the build —
    /// useful in crash reports and bug filings, not displayed by default.
    pub build_tag: String,
    pub tagline: String,
    pub website: String,
    pub website_label: String,
    pub repo_url: String,
    pub discord_url: String,
    pub license: String,
    pub license_name: String,
    pub license_url: String,
    pub copyright: String,
    /// `(name, role)` pairs
    pub authors: Vec<[String; 2]>,
    pub acknowledgements: String,
    /// PNG data URL (`data:image/png;base64,…`) of the Tauri app icon, or
    /// `None` if the icon could not be read (should never happen in practice).
    pub icon_data_url: Option<String>,
}

// ── Icon encoding ─────────────────────────────────────────────────────────────

/// Encode the highest-resolution app icon as a `data:image/png;base64,…` URL.
///
/// `tauri::include_image!` embeds `icons/icon.png` (512 × 512) directly into
/// the binary at compile time, so the About window always shows the full-
/// resolution asset rather than whatever smaller size the OS chose for the
/// window chrome via `default_window_icon()`.
///
/// Tauri's `Image` stores raw RGBA pixels; we re-encode them to PNG in memory
/// so the frontend can use the result directly in an `<img src>`.
fn icon_data_url() -> Option<String> {
    let icon = tauri::include_image!("icons/icon.png");
    let width = icon.width();
    let height = icon.height();
    let rgba = icon.rgba();

    let mut png_bytes: Vec<u8> = Vec::new();
    let mut encoder = png::Encoder::new(&mut png_bytes, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header().ok()?.write_image_data(rgba).ok()?;

    Some(format!(
        "data:image/png;base64,{}",
        STANDARD.encode(&png_bytes)
    ))
}

// ── Tauri commands ────────────────────────────────────────────────────────────

/// Return all about-page data to the frontend in one call.
#[tauri::command]
pub fn get_about_info(app: AppHandle) -> AboutInfo {
    let version = app.package_info().version.to_string();
    let display_version = strip_rc_suffix(&version);
    let channel = crate::update_channel::read_channel(&app)
        .as_str()
        .to_string();
    let commit_full = env!("BUILD_INFO_COMMIT");
    let commit_short = if commit_full.len() >= 7 {
        commit_full[..7].to_string()
    } else {
        commit_full.to_string()
    };
    let build_tag = env!("BUILD_INFO_TAG").to_string();

    AboutInfo {
        name: APP_DISPLAY_NAME.into(),
        version,
        display_version,
        channel,
        commit_short,
        build_tag,
        tagline: APP_TAGLINE.into(),
        website: APP_WEBSITE.into(),
        website_label: APP_WEBSITE_LABEL.into(),
        repo_url: APP_REPO_URL.into(),
        discord_url: APP_DISCORD_URL.into(),
        license: APP_LICENSE.into(),
        license_name: APP_LICENSE_NAME.into(),
        license_url: APP_LICENSE_URL.into(),
        copyright: APP_COPYRIGHT.into(),
        authors: APP_AUTHORS
            .iter()
            .map(|(n, r)| [n.to_string(), r.to_string()])
            .collect(),
        acknowledgements: APP_ACKNOWLEDGEMENTS.into(),
        icon_data_url: icon_data_url(),
    }
}

/// Strip a SemVer pre-release suffix matching `-rc.N` from the version
/// string. Anything else is returned unchanged. Bit-identical promotion
/// means `0.5.1-rc.3` is what stable users actually run, so the About
/// page normalises it for display.
fn strip_rc_suffix(version: &str) -> String {
    match version.find("-rc.") {
        Some(i) => version[..i].to_string(),
        None => version.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::strip_rc_suffix;

    #[test]
    fn strips_rc_suffix() {
        assert_eq!(strip_rc_suffix("0.5.1-rc.3"), "0.5.1");
        assert_eq!(strip_rc_suffix("0.5.1-rc.42"), "0.5.1");
        assert_eq!(strip_rc_suffix("0.5.1"), "0.5.1");
        assert_eq!(strip_rc_suffix("1.0.0"), "1.0.0");
    }
}

/// Open (or focus) the custom About window.
#[tauri::command]
pub async fn open_about_window(app: AppHandle) -> Result<(), String> {
    let title = format!("About {APP_DISPLAY_NAME}");
    crate::window_cmds::focus_or_create(
        &app,
        crate::window_cmds::WindowSpec {
            label: "about",
            route: "about",
            title: &title,
            inner_size: (520.0, 740.0),
            resizable: false,
            ..Default::default()
        },
    )
}
