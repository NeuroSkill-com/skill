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
use tauri::{AppHandle, Manager};

use crate::constants::{
    APP_DISPLAY_NAME, APP_TAGLINE,
    APP_WEBSITE, APP_WEBSITE_LABEL, APP_REPO_URL,
    APP_LICENSE, APP_LICENSE_NAME, APP_LICENSE_URL,
    APP_COPYRIGHT, APP_AUTHORS, APP_ACKNOWLEDGEMENTS,
};

// ── Serialisable about payload ────────────────────────────────────────────────

/// All about-page data in one serialisable blob.
/// The frontend receives this via [`get_about_info`].
#[derive(serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AboutInfo {
    pub name:             String,
    pub version:          String,
    pub tagline:          String,
    pub website:          String,
    pub website_label:    String,
    pub repo_url:         String,
    pub license:          String,
    pub license_name:     String,
    pub license_url:      String,
    pub copyright:        String,
    /// `(name, role)` pairs
    pub authors:          Vec<[String; 2]>,
    pub acknowledgements: String,
    /// PNG data URL (`data:image/png;base64,…`) of the Tauri app icon, or
    /// `None` if the icon could not be read (should never happen in practice).
    pub icon_data_url:    Option<String>,
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
    let icon   = tauri::include_image!("icons/icon.png");
    let width  = icon.width();
    let height = icon.height();
    let rgba   = icon.rgba();

    let mut png_bytes: Vec<u8> = Vec::new();
    let mut encoder = png::Encoder::new(&mut png_bytes, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header().ok()?.write_image_data(rgba).ok()?;

    Some(format!("data:image/png;base64,{}", STANDARD.encode(&png_bytes)))
}

// ── Tauri commands ────────────────────────────────────────────────────────────

/// Return all about-page data to the frontend in one call.
#[tauri::command]
pub fn get_about_info(app: AppHandle) -> AboutInfo {
    let version = app.package_info().version.to_string();
    AboutInfo {
        name:             APP_DISPLAY_NAME.into(),
        version,
        tagline:          APP_TAGLINE.into(),
        website:          APP_WEBSITE.into(),
        website_label:    APP_WEBSITE_LABEL.into(),
        repo_url:         APP_REPO_URL.into(),
        license:          APP_LICENSE.into(),
        license_name:     APP_LICENSE_NAME.into(),
        license_url:      APP_LICENSE_URL.into(),
        copyright:        APP_COPYRIGHT.into(),
        authors:          APP_AUTHORS
                              .iter()
                              .map(|(n, r)| [n.to_string(), r.to_string()])
                              .collect(),
        acknowledgements: APP_ACKNOWLEDGEMENTS.into(),
        icon_data_url:    icon_data_url(),
    }
}

/// Open (or focus) the custom About window.
#[tauri::command]
pub async fn open_about_window(app: AppHandle) -> Result<(), String> {
    if let Some(win) = app.get_webview_window("about") {
        let _ = win.unminimize();
        let _ = win.show();
        let _ = win.set_focus();
        return Ok(());
    }
    tauri::WebviewWindowBuilder::new(
        &app,
        "about",
        tauri::WebviewUrl::App("about".into()),
    )
    .title(format!("About {APP_DISPLAY_NAME}"))
    .inner_size(520.0, 740.0)
    .resizable(false)
    .center()
    .decorations(false).transparent(true)
    .build()
    .map(|w| { let _ = w.set_focus(); })
    .map_err(|e| e.to_string())
}


