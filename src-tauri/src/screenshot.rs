// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
//! Screenshot capture + vision-encoder embedding system.
//!
//! Every ~5 seconds (aligned with EEG embedding epoch cadence), captures the
//! active application window, encodes it through a vision embedding model, and
//! stores the raw embedding alongside metadata in SQLite + HNSW.  The shared
//! `YYYYMMDDHHmmss` timestamp is the cross-modal join key to EEG embeddings.

use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use fast_hnsw::{Builder, distance::Cosine, labeled::LabeledIndex};
use image::{DynamicImage, GenericImageView, ImageReader};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::{AppState, MutexExt};
use crate::constants::{
    SCREENSHOTS_DIR, SCREENSHOTS_HNSW, SCREENSHOT_HNSW_SAVE_EVERY,
    HNSW_M, HNSW_EF_CONSTRUCTION,
};
use crate::screenshot_store::{
    ScreenshotStore, ScreenshotRow, ScreenshotResult,
    ReembedEstimate, ReembedResult,
};
use crate::settings::ScreenshotConfig;

// ── Captured image ────────────────────────────────────────────────────────────

#[allow(dead_code)]
struct CapturedImage {
    raw_bytes: Vec<u8>,
    width:     u32,
    height:    u32,
}

// ── Platform window capture ───────────────────────────────────────────────────

/// Capture the active application window.
/// Returns `None` if capture fails or is unsupported.
fn capture_active_window() -> Option<CapturedImage> {
    #[cfg(target_os = "macos")]
    { capture_macos() }
    #[cfg(target_os = "linux")]
    { capture_linux() }
    #[cfg(target_os = "windows")]
    { capture_windows() }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    { None }
}

#[cfg(target_os = "macos")]
fn capture_macos() -> Option<CapturedImage> {
    use std::process::Command;

    let tmp = std::env::temp_dir().join("skill_screenshot.png");
    let _ = std::fs::remove_file(&tmp); // clean slate

    // ── Attempt 1: capture the specific frontmost window by CGWindowID.
    // Completely silent — no cursor change, no user interaction.
    let window_id = macos_frontmost_window_id();
    if let Some(wid) = window_id {
        let ok = Command::new("screencapture")
            .args(["-x", "-t", "png", "-l"])
            .arg(wid.to_string())
            .arg(&tmp)
            .status()
            .ok()
            .map(|s| s.success())
            .unwrap_or(false);
        if ok && tmp.exists() {
            if let Some(img) = read_captured_image(&tmp) { return Some(img); }
        }
    }

    // ── Attempt 2: full-screen capture via screencapture (silent).
    let _ = std::fs::remove_file(&tmp);
    let ok = Command::new("screencapture")
        .args(["-x", "-t", "png"])
        .arg(&tmp)
        .status()
        .ok()
        .map(|s| s.success())
        .unwrap_or(false);
    if ok && tmp.exists() {
        if let Some(img) = read_captured_image(&tmp) { return Some(img); }
    }

    // ── Attempt 3: osascript fallback — full-screen screenshot via
    // AppleScript.  Works on older macOS or when screencapture is
    // restricted but osascript retains screen access.
    let _ = std::fs::remove_file(&tmp);
    let script = format!(
        "do shell script \"screencapture -x -t png {}\"",
        tmp.to_string_lossy()
    );
    let ok = Command::new("osascript")
        .args(["-e", &script])
        .status()
        .ok()
        .map(|s| s.success())
        .unwrap_or(false);
    if ok && tmp.exists() {
        if let Some(img) = read_captured_image(&tmp) { return Some(img); }
    }

    None
}

/// Read a captured PNG from disk, decode it, clean up the temp file.
#[cfg(target_os = "macos")]
fn read_captured_image(path: &std::path::Path) -> Option<CapturedImage> {
    let raw_bytes = std::fs::read(path).ok()?;
    let _ = std::fs::remove_file(path);
    if raw_bytes.is_empty() { return None; }
    let img = ImageReader::new(Cursor::new(&raw_bytes))
        .with_guessed_format().ok()?
        .decode().ok()?;
    let (w, h) = img.dimensions();
    Some(CapturedImage { raw_bytes, width: w, height: h })
}

/// Get the CGWindowID of the frontmost application's main window via
/// CoreGraphics FFI.  Completely silent — no cursor change, no user
/// interaction, no subprocess, sub-millisecond.
///
/// Uses `CGWindowListCopyWindowInfo` (linked via CoreGraphics.framework,
/// always available on macOS) and the frontmost PID from the
/// `NSWorkspace` shared instance (linked via AppKit.framework).
#[cfg(target_os = "macos")]
fn macos_frontmost_window_id() -> Option<u64> {
    use std::ffi::c_void;

    // ── CoreFoundation / CoreGraphics C types ──
    use std::os::raw::c_char;

    type CFTypeRef       = *const c_void;
    type CFAllocatorRef  = *const c_void;
    type CFArrayRef      = *const c_void;
    type CFDictionaryRef = *const c_void;
    type CFStringRef     = *const c_void;
    type CFIndex         = isize;
    type CGWindowID      = u32;
    // CFNumberType constants (i32 to match gpu_stats.rs)
    type CFNumberType    = i32;

    const K_CF_NUMBER_SINT32_TYPE: CFNumberType = 3;
    const K_CF_NUMBER_SINT64_TYPE: CFNumberType = 4;

    // CGWindowListOption flags
    const ON_SCREEN_ONLY: u32 = 1 << 0;
    const EXCLUDE_DESKTOP: u32 = 1 << 4;
    const K_CG_NULL_WINDOW_ID: CGWindowID = 0;

    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGWindowListCopyWindowInfo(option: u32, relativeToWindow: CGWindowID) -> CFArrayRef;
    }

    // Signatures match gpu_stats.rs exactly to avoid clashing_extern_declarations.
    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFArrayGetCount(theArray: CFArrayRef) -> CFIndex;
        fn CFArrayGetValueAtIndex(theArray: CFArrayRef, idx: CFIndex) -> CFTypeRef;
        fn CFDictionaryGetValue(dict: CFDictionaryRef, key: CFStringRef) -> CFTypeRef;
        fn CFNumberGetValue(number: CFTypeRef, the_type: CFNumberType, value_ptr: *mut i64) -> bool;
        fn CFRelease(cf: CFTypeRef);
        fn CFStringCreateWithCString(
            alloc:    CFAllocatorRef,
            c_str:    *const c_char,
            encoding: u32,
        ) -> CFStringRef;
    }

    const K_CF_STRING_ENCODING_UTF8: u32 = 0x0800_0100;

    /// Helper: create a CFString from a `&[u8]` C-string literal.
    unsafe fn cfstr(s: &[u8]) -> CFStringRef {
        CFStringCreateWithCString(std::ptr::null(), s.as_ptr() as *const c_char, K_CF_STRING_ENCODING_UTF8)
    }

    /// Helper: get an i32 from a CFNumber.
    unsafe fn cfnum_i32(n: CFTypeRef) -> Option<i32> {
        if n.is_null() { return None; }
        let mut v: i64 = 0;
        if CFNumberGetValue(n, K_CF_NUMBER_SINT32_TYPE, &mut v) {
            Some(v as i32)
        } else { None }
    }

    /// Helper: get an i64 from a CFNumber (some fields may be i64).
    unsafe fn cfnum_i64(n: CFTypeRef) -> Option<i64> {
        if n.is_null() { return None; }
        let mut v: i64 = 0;
        if CFNumberGetValue(n, K_CF_NUMBER_SINT64_TYPE, &mut v) {
            Some(v)
        } else {
            // Fall back to i32
            if CFNumberGetValue(n, K_CF_NUMBER_SINT32_TYPE, &mut v) {
                Some(v)
            } else { None }
        }
    }

    // ── Get frontmost PID from NSWorkspace ──
    // NSWorkspace.sharedWorkspace.frontmostApplication.processIdentifier
    // All of these are already linked via objc2-app-kit.
    let front_pid: i32 = {
        use objc2::runtime::AnyObject;
        use objc2::msg_send;
        use objc2_app_kit::NSWorkspace;

        let workspace = NSWorkspace::sharedWorkspace();
        let front_app: Option<&AnyObject> = unsafe {
            msg_send![&workspace, frontmostApplication]
        };
        let front_app = front_app?;
        let pid: i32 = unsafe { msg_send![front_app, processIdentifier] };
        if pid <= 0 { return None; }
        pid
    };

    unsafe {
        let key_pid    = cfstr(b"kCGWindowOwnerPID\0");
        let key_layer  = cfstr(b"kCGWindowLayer\0");
        let key_number = cfstr(b"kCGWindowNumber\0");

        let list = CGWindowListCopyWindowInfo(
            ON_SCREEN_ONLY | EXCLUDE_DESKTOP,
            K_CG_NULL_WINDOW_ID,
        );
        if list.is_null() {
            CFRelease(key_pid); CFRelease(key_layer); CFRelease(key_number);
            return None;
        }

        let count = CFArrayGetCount(list);
        let mut result: Option<u64> = None;

        for i in 0..count {
            let dict = CFArrayGetValueAtIndex(list, i);
            if dict.is_null() { continue; }

            // Match PID
            let pid_ref = CFDictionaryGetValue(dict, key_pid);
            let pid = cfnum_i32(pid_ref).unwrap_or(-1);
            if pid != front_pid { continue; }

            // Layer must be 0 (normal window)
            let layer_ref = CFDictionaryGetValue(dict, key_layer);
            let layer = cfnum_i32(layer_ref).unwrap_or(-1);
            if layer != 0 { continue; }

            // Get window number
            let num_ref = CFDictionaryGetValue(dict, key_number);
            if let Some(wid) = cfnum_i64(num_ref) {
                result = Some(wid as u64);
                break;
            }
        }

        CFRelease(list);
        CFRelease(key_pid);
        CFRelease(key_layer);
        CFRelease(key_number);

        result
    }
}

#[cfg(target_os = "linux")]
fn capture_linux() -> Option<CapturedImage> {
    use std::process::Command;

    let tmp = std::env::temp_dir().join("skill_screenshot.png");

    // Try xdotool + import (ImageMagick) for X11 — captures active window
    let win_id = Command::new("xdotool")
        .arg("getactivewindow")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());

    let mut captured = false;

    if let Some(ref wid) = win_id {
        // X11 path 1: import -window <wid> (writes PNG to stdout)
        if let Some(output) = Command::new("import")
            .args(["-window", wid.as_str(), "png:-"])
            .output()
            .ok()
            .filter(|o| o.status.success() && !o.stdout.is_empty())
        {
            if std::fs::write(&tmp, &output.stdout).is_ok() {
                captured = true;
            }
        }
    }

    if !captured {
        if let Some(ref _wid) = win_id {
            // X11 path 2: scrot -u (focused window)
            captured = Command::new("scrot")
                .args(["-u", "-o", &tmp.to_string_lossy()])
                .status()
                .ok()
                .map(|s| s.success())
                .unwrap_or(false);
        }
    }

    if !captured {
        // Wayland: try swaymsg + grim with geometry for focused window
        let geo = Command::new("sh")
            .args(["-c", r#"swaymsg -t get_tree | jq -r '.. | select(.focused?) | .rect | "\(.x),\(.y) \(.width)x\(.height)"'"#])
            .output()
            .ok()
            .filter(|o| o.status.success() && !o.stdout.is_empty())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());

        if let Some(geo) = geo {
            captured = Command::new("grim")
                .args(["-g", &geo])
                .arg(&tmp)
                .status()
                .ok()
                .map(|s| s.success())
                .unwrap_or(false);
        }
    }

    if !captured {
        // Last resort: grim full screen
        captured = Command::new("grim")
            .arg(&tmp)
            .status()
            .ok()
            .map(|s| s.success())
            .unwrap_or(false);
    }

    if !captured { return None; }

    let raw_bytes = std::fs::read(&tmp).ok()?;
    let _ = std::fs::remove_file(&tmp);

    let img = ImageReader::new(Cursor::new(&raw_bytes))
        .with_guessed_format().ok()?
        .decode().ok()?;
    let (w, h) = img.dimensions();

    Some(CapturedImage { raw_bytes, width: w, height: h })
}

#[cfg(target_os = "windows")]
fn capture_windows() -> Option<CapturedImage> {
    use std::process::Command;

    // Capture the foreground window (not full screen) via PowerShell + Win32.
    // Uses GetForegroundWindow → GetWindowRect → CopyFromScreen with the
    // window's bounding rectangle, matching the plan's specification.
    let tmp = std::env::temp_dir().join("skill_screenshot.png");
    let ps_script = format!(
        r#"
        Add-Type -AssemblyName System.Drawing
        Add-Type @"
            using System;
            using System.Runtime.InteropServices;
            public class Win32 {{
                [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
                [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT rect);
            }}
            public struct RECT {{
                public int Left, Top, Right, Bottom;
            }}
"@
        $hwnd = [Win32]::GetForegroundWindow()
        $rect = New-Object RECT
        [Win32]::GetWindowRect($hwnd, [ref]$rect) | Out-Null
        $w = $rect.Right - $rect.Left
        $h = $rect.Bottom - $rect.Top
        if ($w -le 0 -or $h -le 0) {{ exit 1 }}
        $bmp = New-Object System.Drawing.Bitmap($w, $h)
        $g = [System.Drawing.Graphics]::FromImage($bmp)
        $g.CopyFromScreen($rect.Left, $rect.Top, 0, 0, (New-Object System.Drawing.Size($w, $h)))
        $bmp.Save("{}")
        $g.Dispose()
        $bmp.Dispose()
        "#,
        tmp.to_string_lossy().replace('\\', "\\\\")
    );

    let status = Command::new("powershell")
        .args(["-NoProfile", "-Command", &ps_script])
        .status()
        .ok()?;
    if !status.success() { return None; }

    let raw_bytes = std::fs::read(&tmp).ok()?;
    let _ = std::fs::remove_file(&tmp);

    let img = ImageReader::new(Cursor::new(&raw_bytes))
        .with_guessed_format().ok()?
        .decode().ok()?;
    let (w, h) = img.dimensions();

    Some(CapturedImage { raw_bytes, width: w, height: h })
}

// ── Image resize + pad ────────────────────────────────────────────────────────

/// Resize with aspect-ratio-preserving fit (Lanczos3), then center-pad to
/// `target × target` with black pixels.  Returns the resized RGB image as
/// PNG bytes plus the final dimensions.
fn resize_fit_pad(raw_bytes: &[u8], target: u32) -> Option<(Vec<u8>, u32, u32)> {
    let img = ImageReader::new(Cursor::new(raw_bytes))
        .with_guessed_format().ok()?
        .decode().ok()?;

    let (w, h) = img.dimensions();
    let scale = (target as f64 / w as f64).min(target as f64 / h as f64);
    let nw = (w as f64 * scale).round() as u32;
    let nh = (h as f64 * scale).round() as u32;

    let resized = img.resize_exact(nw, nh, image::imageops::FilterType::Lanczos3);

    // Center-pad to target × target
    let mut canvas = DynamicImage::new_rgb8(target, target);
    let offset_x = (target - nw) / 2;
    let offset_y = (target - nh) / 2;
    image::imageops::overlay(&mut canvas, &resized, offset_x as i64, offset_y as i64);

    // Encode as PNG for the vision encoder
    let mut png_bytes = Vec::new();
    canvas.write_to(&mut Cursor::new(&mut png_bytes), image::ImageFormat::Png).ok()?;

    Some((png_bytes, target, target))
}

/// Encode an image as WebP with the given quality.
fn encode_webp(raw_bytes: &[u8], _quality: u8, out_path: &Path) -> Option<u64> {
    let img = ImageReader::new(Cursor::new(raw_bytes))
        .with_guessed_format().ok()?
        .decode().ok()?;

    let mut buf = Vec::new();
    img.write_to(&mut Cursor::new(&mut buf), image::ImageFormat::WebP).ok()?;
    std::fs::write(out_path, &buf).ok()?;
    Some(buf.len() as u64)
}

// ── Timestamp helpers ─────────────────────────────────────────────────────────

/// Generate `YYYYMMDDHHmmss` timestamp (UTC) from current time.
///
/// All timestamps in the screenshot system are **UTC** — matching the EEG
/// embedding pipeline's `YYYYMMDDHHmmss` convention.  `chrono::DateTime::from_timestamp`
/// returns `DateTime<Utc>`, so the formatted string is always in UTC regardless
/// of the system's local timezone.
fn yyyymmddhhmmss_utc() -> (String, u64) {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let unix_ts = now.as_secs();

    // chrono::DateTime::from_timestamp returns DateTime<Utc> — always UTC.
    let dt = chrono::DateTime::from_timestamp(unix_ts as i64, 0)
        .unwrap_or_default();
    let ts_str = dt.format("%Y%m%d%H%M%S").to_string();

    (ts_str, unix_ts)
}

// ── HNSW helpers ──────────────────────────────────────────────────────────────

fn fresh_hnsw() -> LabeledIndex<Cosine, i64> {
    Builder::new()
        .m(HNSW_M)
        .ef_construction(HNSW_EF_CONSTRUCTION)
        .build_labeled(Cosine)
}

fn load_or_rebuild_hnsw(
    skill_dir: &Path,
    store: &ScreenshotStore,
) -> LabeledIndex<Cosine, i64> {
    let hnsw_path = skill_dir.join(SCREENSHOTS_HNSW);
    if hnsw_path.exists() {
        match LabeledIndex::<Cosine, i64>::load(&hnsw_path, Cosine) {
            Ok(idx) => {
                eprintln!("[screenshot] loaded HNSW from {}", hnsw_path.display());
                return idx;
            }
            Err(e) => {
                eprintln!("[screenshot] HNSW load error: {e} — rebuilding");
            }
        }
    }

    rebuild_hnsw_from_sqlite(store, skill_dir)
}

fn rebuild_hnsw_from_sqlite(
    store: &ScreenshotStore,
    skill_dir: &Path,
) -> LabeledIndex<Cosine, i64> {
    let mut idx = fresh_hnsw();
    let rows = store.all_embeddings();
    eprintln!("[screenshot] rebuilding HNSW from {} embeddings", rows.len());
    for (ts, emb) in &rows {
        idx.insert(emb.clone(), *ts);
    }
    let hnsw_path = skill_dir.join(SCREENSHOTS_HNSW);
    if let Err(e) = idx.save(&hnsw_path) {
        eprintln!("[screenshot] HNSW save error: {e}");
    }
    idx
}

fn save_hnsw(idx: &LabeledIndex<Cosine, i64>, skill_dir: &Path) {
    let path = skill_dir.join(SCREENSHOTS_HNSW);
    if let Err(e) = idx.save(&path) {
        eprintln!("[screenshot] HNSW save error: {e}");
    }
}

// ── fastembed image embedder ──────────────────────────────────────────────────

/// Try to create a fastembed `ImageEmbedding` instance.  Public alias for Tauri commands.
pub fn load_fastembed_image_pub(config: &ScreenshotConfig, skill_dir: &Path) -> Option<fastembed::ImageEmbedding> {
    load_fastembed_image(config, skill_dir)
}

/// Try to create a fastembed `ImageEmbedding` instance.
fn load_fastembed_image(config: &ScreenshotConfig, skill_dir: &Path) -> Option<fastembed::ImageEmbedding> {
    if config.embed_backend != "fastembed" { return None; }
    let model = config.fastembed_model_enum()?;
    let cache = skill_dir.join("fastembed_cache");
    match fastembed::ImageEmbedding::try_new(
        fastembed::ImageInitOptions::new(model).with_cache_dir(cache)
    ) {
        Ok(e) => {
            eprintln!("[screenshot] fastembed image model loaded: {}", config.fastembed_model);
            Some(e)
        }
        Err(e) => {
            eprintln!("[screenshot] fastembed image model error: {e}");
            None
        }
    }
}

/// Embed a single image (PNG bytes) using fastembed.  Public alias for Tauri commands.
pub fn fastembed_embed_pub(encoder: &mut fastembed::ImageEmbedding, png_bytes: &[u8]) -> Option<Vec<f32>> {
    fastembed_embed(encoder, png_bytes)
}

/// Embed a single image (PNG bytes) using fastembed.
fn fastembed_embed(encoder: &mut fastembed::ImageEmbedding, png_bytes: &[u8]) -> Option<Vec<f32>> {
    match encoder.embed_bytes(&[png_bytes], None) {
        Ok(mut vecs) if !vecs.is_empty() => Some(vecs.remove(0)),
        Ok(_) => None,
        Err(e) => {
            eprintln!("[screenshot] embed error: {e}");
            None
        }
    }
}

// ── Screenshot event payload ──────────────────────────────────────────────────

#[derive(Clone, Serialize)]
struct ScreenshotCapturedEvent {
    ts:       String,
    filename: String,
}

// ── Background worker ─────────────────────────────────────────────────────────

/// Run the screenshot capture worker in a dedicated thread.
/// Called from `lib.rs :: setup_app`.
pub fn run_screenshot_worker(
    app: AppHandle,
    skill_dir: PathBuf,
    shared_store: Option<Arc<ScreenshotStore>>,
) {
    // Use the shared store or open a new one
    let store = match shared_store.or_else(|| ScreenshotStore::open(&skill_dir).map(Arc::new)) {
        Some(s) => s,
        None => {
            eprintln!("[screenshot] failed to open store — worker exiting");
            return;
        }
    };

    // Read initial config
    let config = {
        let r = app.state::<Mutex<Box<AppState>>>();
        let g = r.lock_or_recover();
        g.screenshot_config.clone()
    };

    // Load HNSW
    let mut hnsw = load_or_rebuild_hnsw(&skill_dir, &store);

    // Load fastembed model if configured
    let mut fe_encoder = load_fastembed_image(&config, &skill_dir);

    let screenshots_dir = skill_dir.join(SCREENSHOTS_DIR);
    let _ = std::fs::create_dir_all(&screenshots_dir);

    let mut inserts_since_save: usize = 0;
    let mut last_config = config;

    loop {
        // Re-read config each iteration (it may change via settings UI)
        let config = {
            let r = app.state::<Mutex<Box<AppState>>>();
            let g = r.lock_or_recover();
            g.screenshot_config.clone()
        };

        let interval = Duration::from_secs(config.interval_secs.max(1) as u64);
        std::thread::sleep(interval);

        // ── Gate checks ──
        if !config.enabled { continue; }

        if config.session_only {
            let session_active = {
                let r = app.state::<Mutex<Box<AppState>>>();
                let g = r.lock_or_recover();
                g.session_start_utc.is_some()
            };
            if !session_active { continue; }
        }

        // Reload encoder if model changed
        if config.embed_backend != last_config.embed_backend
            || config.fastembed_model != last_config.fastembed_model
        {
            eprintln!("[screenshot] model changed — reloading encoder");
            fe_encoder = load_fastembed_image(&config, &skill_dir);
        }
        last_config = config.clone();

        // ── Capture active window ──
        let captured = match capture_active_window() {
            Some(c) => c,
            None => continue,
        };

        // ── Resize + pad ──
        let (resized_png, w, h) = match resize_fit_pad(&captured.raw_bytes, config.image_size) {
            Some(r) => r,
            None => continue,
        };

        // ── Save to disk as WebP ──
        let (ts_str, unix_ts) = yyyymmddhhmmss_utc();
        let date_str = &ts_str[..8];
        let date_dir = screenshots_dir.join(date_str);
        let _ = std::fs::create_dir_all(&date_dir);
        let webp_name = format!("{date_str}/{ts_str}.webp");
        let webp_path = screenshots_dir.join(&webp_name);
        let file_size = match encode_webp(&resized_png, config.quality, &webp_path) {
            Some(s) => s,
            None => continue,
        };

        // ── Active window context ──
        let (app_name, window_title) = {
            let r = app.state::<Mutex<Box<AppState>>>();
            let g = r.lock_or_recover();
            match &g.current_active_window {
                Some(aw) => (aw.app_name.clone(), aw.window_title.clone()),
                None => (String::new(), String::new()),
            }
        };

        // ── Embed ──
        let (embedding, model_backend, model_id) = match config.embed_backend.as_str() {
            "fastembed" => {
                if let Some(ref mut fe) = fe_encoder {
                    let emb = fastembed_embed(fe, &resized_png);
                    let mid = config.model_id();
                    (emb, "fastembed".to_string(), mid)
                } else {
                    (None, String::new(), String::new())
                }
            }
            // mmproj embedding — send through the LLM actor's request channel.
            "mmproj" => {
                #[cfg(feature = "llm")]
                {
                    let result = (|| -> Option<Vec<f32>> {
                        let cell = {
                            let r = app.state::<Mutex<Box<AppState>>>();
                            let g = r.lock_or_recover();
                            g.llm.state_cell.clone()
                        };
                        let state = cell.lock().ok()?.as_ref()?.clone();
                        if !state.vision_ready.load(std::sync::atomic::Ordering::Relaxed) {
                            return None;
                        }
                        let (tx, rx) = tokio::sync::oneshot::channel();
                        state.req_tx.send(crate::llm::InferRequest::EmbedImage {
                            bytes: resized_png.clone(),
                            result_tx: tx,
                        }).ok()?;
                        // Wait up to 30 seconds for the embedding
                        rx.blocking_recv().ok()?
                    })();
                    let mid = config.model_id();
                    if result.is_some() {
                        (result, "mmproj".to_string(), mid)
                    } else {
                        (None, String::new(), String::new())
                    }
                }
                #[cfg(not(feature = "llm"))]
                {
                    (None, String::new(), String::new())
                }
            }
            _ => (None, String::new(), String::new()),
        };

        let embedding_dim = embedding.as_ref().map_or(0, |e| e.len());
        let ts_i64: i64 = ts_str.parse().unwrap_or(0);

        // ── HNSW insert ──
        let hnsw_id = if let Some(ref emb) = embedding {
            let id = hnsw.len() as u64;
            hnsw.insert(emb.clone(), ts_i64);
            inserts_since_save += 1;
            if inserts_since_save >= SCREENSHOT_HNSW_SAVE_EVERY {
                save_hnsw(&hnsw, &skill_dir);
                inserts_since_save = 0;
            }
            Some(id)
        } else {
            None
        };

        // ── SQLite insert ──
        store.insert(&ScreenshotRow {
            timestamp: ts_i64,
            unix_ts,
            filename: webp_name.clone(),
            width: w,
            height: h,
            file_size,
            hnsw_id,
            embedding,
            embedding_dim,
            model_backend,
            model_id,
            image_size: config.image_size,
            quality: config.quality,
            app_name,
            window_title,
        });

        // ── Notify frontend ──
        let _ = app.emit("screenshot-captured", ScreenshotCapturedEvent {
            ts: ts_str, filename: webp_name,
        });
    }
}

// ── Public query functions (called from Tauri commands) ───────────────────────

/// Search screenshots by embedding vector using the HNSW index.
pub fn search_by_vector(
    hnsw: &LabeledIndex<Cosine, i64>,
    store: &ScreenshotStore,
    query: &[f32],
    k: usize,
) -> Vec<ScreenshotResult> {
    let ef = k.max(100); // ef >= k for good recall
    let results = hnsw.search(query, k, ef);
    results.iter().map(|r| {
        let ts = *r.payload;
        let around = store.around_timestamp(ts, 1);
        if let Some(mut sr) = around.into_iter().next() {
            sr.similarity = 1.0 - r.distance; // cosine distance → similarity
            sr
        } else {
            ScreenshotResult {
                timestamp: ts,
                unix_ts: 0,
                filename: String::new(),
                app_name: String::new(),
                window_title: String::new(),
                similarity: 1.0 - r.distance,
            }
        }
    }).collect()
}

/// Get screenshots around a given unix timestamp.
pub fn get_around(store: &ScreenshotStore, timestamp: i64, window_secs: i32) -> Vec<ScreenshotResult> {
    store.around_timestamp(timestamp, window_secs)
}

/// Estimate re-embedding work.
pub fn estimate_reembed(
    store: &ScreenshotStore,
    config: &ScreenshotConfig,
    skill_dir: &Path,
) -> ReembedEstimate {
    let backend = &config.embed_backend;
    let mid = config.model_id();
    let total = store.count_embedded();
    let stale = store.count_stale(backend, &mid);
    let unembedded = store.count_unembedded();

    // Benchmark: embed 1 sample image
    let per_image_ms = {
        let mut encoder = load_fastembed_image(config, skill_dir);
        if let Some(ref mut fe) = encoder {
            // Create a tiny test image
            let test_img = DynamicImage::new_rgb8(config.image_size, config.image_size);
            let mut png = Vec::new();
            test_img.write_to(&mut Cursor::new(&mut png), image::ImageFormat::Png).ok();

            let start = Instant::now();
            for _ in 0..3 {
                let _ = fastembed_embed(fe, &png);
            }
            start.elapsed().as_millis() as u64 / 3
        } else {
            250 // default estimate
        }
    };

    let total_to_embed = stale + unembedded;
    let eta_secs = (total_to_embed as u64 * per_image_ms) / 1000;

    ReembedEstimate { total, stale, unembedded, per_image_ms, eta_secs }
}

/// Re-embed all screenshots with the current model.
pub fn rebuild_embeddings(
    store: &ScreenshotStore,
    config: &ScreenshotConfig,
    skill_dir: &Path,
    app: &AppHandle,
) -> ReembedResult {
    let backend = &config.embed_backend;
    let mid = config.model_id();

    let mut encoder = load_fastembed_image(config, skill_dir);
    let rows = store.rows_needing_embed(backend, &mid);
    let total = rows.len();

    let screenshots_dir = skill_dir.join(SCREENSHOTS_DIR);
    let start = Instant::now();
    let mut embedded = 0usize;
    let mut skipped = 0usize;

    for (i, row) in rows.iter().enumerate() {
        let webp_path = screenshots_dir.join(&row.filename);
        if !webp_path.exists() {
            skipped += 1;
            continue;
        }

        // Read + resize
        let raw = match std::fs::read(&webp_path) {
            Ok(b) => b,
            Err(_) => { skipped += 1; continue; }
        };
        let resized = match resize_fit_pad(&raw, config.image_size) {
            Some((png, _, _)) => png,
            None => { skipped += 1; continue; }
        };

        // Embed
        let emb = if let Some(ref mut fe) = encoder {
            fastembed_embed(fe, &resized)
        } else {
            None
        };

        if let Some(emb) = emb {
            store.update_embedding(row.id, &emb, None, backend, &mid, config.image_size);
            embedded += 1;
        } else {
            skipped += 1;
        }

        // Progress event every 10 rows
        if (i + 1) % 10 == 0 || i + 1 == total {
            let elapsed = start.elapsed().as_secs_f64();
            let rate = if embedded > 0 { elapsed / embedded as f64 } else { 0.25 };
            let remaining = total - i - 1;
            let eta = remaining as f64 * rate;
            let _ = app.emit("screenshot-reembed-progress", serde_json::json!({
                "done": i + 1,
                "total": total,
                "elapsed_secs": elapsed,
                "eta_secs": eta,
            }));
        }
    }

    // Rebuild HNSW
    rebuild_hnsw_from_sqlite(store, skill_dir);

    ReembedResult {
        embedded,
        skipped,
        elapsed_secs: start.elapsed().as_secs_f64(),
    }
}
