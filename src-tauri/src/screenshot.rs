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

use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use fast_hnsw::{Builder, distance::Cosine, labeled::LabeledIndex};
use image::{DynamicImage, GenericImageView, ImageReader};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::{AppState, MutexExt};
use crate::constants::{
    SCREENSHOTS_DIR, SCREENSHOTS_HNSW, SCREENSHOTS_OCR_HNSW, SCREENSHOT_HNSW_SAVE_EVERY,
    HNSW_M, HNSW_EF_CONSTRUCTION,
    OCR_DETECTION_MODEL_URL, OCR_RECOGNITION_MODEL_URL,
    OCR_DETECTION_MODEL_FILE, OCR_RECOGNITION_MODEL_FILE,
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

/// Build execution providers based on the `use_gpu` config flag.
/// On macOS: CoreML (GPU/ANE) when use_gpu=true, CPU-only otherwise.
/// On other platforms: default (CPU) — ort picks the best available.
fn build_execution_providers(use_gpu: bool) -> Vec<ort::execution_providers::ExecutionProviderDispatch> {
    if use_gpu {
        #[cfg(target_os = "macos")]
        {
            eprintln!("[screenshot] using CoreML execution provider (GPU/ANE)");
            vec![ort::ep::CoreML::default().build()]
        }
        #[cfg(not(target_os = "macos"))]
        {
            // On non-macOS, ort defaults to CPU; GPU EPs (CUDA, DirectML)
            // require separate ort features that may not be compiled in.
            eprintln!("[screenshot] using default execution provider");
            vec![]
        }
    } else {
        eprintln!("[screenshot] forcing CPU execution provider");
        vec![ort::ep::CPU::default().build()]
    }
}

/// Try to create a fastembed `ImageEmbedding` instance.  Public alias for Tauri commands.
pub fn load_fastembed_image_pub(config: &ScreenshotConfig, skill_dir: &Path) -> Option<fastembed::ImageEmbedding> {
    load_fastembed_image(config, skill_dir)
}

/// Try to create a fastembed `ImageEmbedding` instance.
fn load_fastembed_image(config: &ScreenshotConfig, skill_dir: &Path) -> Option<fastembed::ImageEmbedding> {
    if config.embed_backend != "fastembed" { return None; }
    let model = config.fastembed_model_enum()?;
    let cache = skill_dir.join("fastembed_cache");
    let eps = build_execution_providers(config.use_gpu);
    match fastembed::ImageEmbedding::try_new(
        fastembed::ImageInitOptions::new(model)
            .with_cache_dir(cache)
            .with_execution_providers(eps)
    ) {
        Ok(e) => {
            eprintln!("[screenshot] fastembed image model loaded: {} (gpu={})",
                config.fastembed_model, config.use_gpu);
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

// ── OCR engine ────────────────────────────────────────────────────────────────

/// Download an OCR model file if it doesn't exist yet. Public alias for Tauri commands.
pub fn download_ocr_model_pub(url: &str, dest: &Path) -> bool {
    download_ocr_model(url, dest)
}

/// Download an OCR model file if it doesn't exist yet.
fn download_ocr_model(url: &str, dest: &Path) -> bool {
    if dest.exists() { return true; }
    eprintln!("[screenshot] downloading OCR model: {url}");
    match ureq::get(url).call() {
        Ok(resp) => {
            let mut body = Vec::new();
            if resp.into_reader().read_to_end(&mut body).is_ok() && !body.is_empty() {
                if let Some(parent) = dest.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                if std::fs::write(dest, &body).is_ok() {
                    eprintln!("[screenshot] OCR model saved: {}", dest.display());
                    return true;
                }
            }
            eprintln!("[screenshot] OCR model download failed (empty body)");
            false
        }
        Err(e) => {
            eprintln!("[screenshot] OCR model download error: {e}");
            false
        }
    }
}

/// Load the ocrs OCR engine.  Downloads model files on first use.
fn load_ocr_engine(skill_dir: &Path) -> Option<ocrs::OcrEngine> {
    let ocr_dir = skill_dir.join("ocr_models");
    let det_path = ocr_dir.join(OCR_DETECTION_MODEL_FILE);
    let rec_path = ocr_dir.join(OCR_RECOGNITION_MODEL_FILE);

    if !download_ocr_model(OCR_DETECTION_MODEL_URL, &det_path) { return None; }
    if !download_ocr_model(OCR_RECOGNITION_MODEL_URL, &rec_path) { return None; }

    let det_model = rten::Model::load_file(&det_path).ok()?;
    let rec_model = rten::Model::load_file(&rec_path).ok()?;

    ocrs::OcrEngine::new(ocrs::OcrEngineParams {
        detection_model: Some(det_model),
        recognition_model: Some(rec_model),
        ..Default::default()
    }).ok()
}

/// Run OCR on raw image bytes (PNG/JPEG/WebP).  Returns the extracted text.
///
/// The image is first downsized to `OCR_MAX_DIMENSION` max dimension
/// (1536px) if larger.  Full retina captures are far too large for the
/// rten-based OCR engine and cause 20+ second inference times.  At 1536px
/// all readable text is preserved and OCR completes in <1 second.
fn run_ocr(engine: &ocrs::OcrEngine, raw_bytes: &[u8]) -> Option<String> {
    let img = image::load_from_memory(raw_bytes).ok()?;
    let (w, h) = img.dimensions();

    // Downsize if larger than OCR_MAX_DIMENSION
    let img = if w > OCR_MAX_DIMENSION || h > OCR_MAX_DIMENSION {
        let scale = (OCR_MAX_DIMENSION as f64 / w as f64)
            .min(OCR_MAX_DIMENSION as f64 / h as f64);
        let nw = (w as f64 * scale).round() as u32;
        let nh = (h as f64 * scale).round() as u32;
        img.resize(nw, nh, image::imageops::FilterType::Triangle)
    } else {
        img
    };

    let rgb = img.into_rgb8();
    let (w, h) = rgb.dimensions();
    let source = ocrs::ImageSource::from_bytes(rgb.as_raw(), (w, h)).ok()?;
    let input = engine.prepare_input(source).ok()?;
    let text = engine.get_text(&input).ok()?;
    let text = text.trim().to_string();
    if text.is_empty() { None } else { Some(text) }
}

/// Embed OCR text using fastembed text embedder with the configured model.
fn embed_ocr_text(text: &str, skill_dir: &Path, config: &ScreenshotConfig) -> Option<Vec<f32>> {
    let cache = skill_dir.join("fastembed_cache");
    let model = config.ocr_text_model_enum();
    let eps = build_execution_providers(config.use_gpu);
    let mut embedder = fastembed::TextEmbedding::try_new(
        fastembed::InitOptions::new(model)
            .with_cache_dir(cache)
            .with_execution_providers(eps)
    ).ok()?;
    let mut results = embedder.embed(vec![text], None).ok()?;
    if results.is_empty() { None } else { Some(results.remove(0)) }
}

// ── OCR HNSW helpers ──────────────────────────────────────────────────────────

fn load_or_rebuild_ocr_hnsw(
    skill_dir: &Path,
    store: &ScreenshotStore,
) -> LabeledIndex<Cosine, i64> {
    let hnsw_path = skill_dir.join(SCREENSHOTS_OCR_HNSW);
    if hnsw_path.exists() {
        match LabeledIndex::<Cosine, i64>::load(&hnsw_path, Cosine) {
            Ok(idx) => {
                eprintln!("[screenshot] loaded OCR HNSW from {}", hnsw_path.display());
                return idx;
            }
            Err(e) => {
                eprintln!("[screenshot] OCR HNSW load error: {e} — rebuilding");
            }
        }
    }
    rebuild_ocr_hnsw_from_sqlite(store, skill_dir)
}

fn rebuild_ocr_hnsw_from_sqlite(
    store: &ScreenshotStore,
    skill_dir: &Path,
) -> LabeledIndex<Cosine, i64> {
    let mut idx = fresh_hnsw();
    let rows = store.all_ocr_embeddings();
    eprintln!("[screenshot] rebuilding OCR HNSW from {} embeddings", rows.len());
    for (ts, emb) in &rows {
        idx.insert(emb.clone(), *ts);
    }
    let hnsw_path = skill_dir.join(SCREENSHOTS_OCR_HNSW);
    if let Err(e) = idx.save(&hnsw_path) {
        eprintln!("[screenshot] OCR HNSW save error: {e}");
    }
    idx
}

fn save_ocr_hnsw(idx: &LabeledIndex<Cosine, i64>, skill_dir: &Path) {
    let path = skill_dir.join(SCREENSHOTS_OCR_HNSW);
    if let Err(e) = idx.save(&path) {
        eprintln!("[screenshot] OCR HNSW save error: {e}");
    }
}

// ── Screenshot event payload ──────────────────────────────────────────────────

#[derive(Clone, Serialize)]
struct ScreenshotCapturedEvent {
    ts:       String,
    filename: String,
}

// ── Pipeline metrics (lock-free atomics) ──────────────────────────────────────

use std::sync::atomic::{AtomicU64, AtomicI64, Ordering};

/// Shared metrics updated by both capture and embed threads.
/// All times are in microseconds.  All counters are monotonic.
pub struct ScreenshotMetrics {
    // ── Capture thread ──
    pub captures:          AtomicU64,
    pub capture_errors:    AtomicU64,
    pub drops:             AtomicU64,   // try_send failures
    pub capture_us:        AtomicU64,   // last window-capture time
    pub ocr_us:            AtomicU64,   // last OCR time
    pub resize_us:         AtomicU64,   // last resize+pad time
    pub save_us:           AtomicU64,   // last WebP save + SQLite insert
    pub capture_total_us:  AtomicU64,   // last full capture-thread iteration

    // ── Embed thread ──
    pub embeds:            AtomicU64,
    pub embed_errors:      AtomicU64,
    pub vision_embed_us:   AtomicU64,   // last vision embedding time
    pub text_embed_us:     AtomicU64,   // last OCR text embedding time
    pub embed_total_us:    AtomicU64,   // last full embed iteration
    pub queue_depth:       AtomicI64,   // current channel occupancy (inc on send, dec on recv)

    // ── Throughput (rolling) ──
    pub last_capture_unix: AtomicU64,   // unix-ms of last capture
    pub last_embed_unix:   AtomicU64,   // unix-ms of last embed completion
}

impl ScreenshotMetrics {
    pub fn new() -> Self {
        Self {
            captures:         AtomicU64::new(0),
            capture_errors:   AtomicU64::new(0),
            drops:            AtomicU64::new(0),
            capture_us:       AtomicU64::new(0),
            ocr_us:           AtomicU64::new(0),
            resize_us:        AtomicU64::new(0),
            save_us:          AtomicU64::new(0),
            capture_total_us: AtomicU64::new(0),
            embeds:           AtomicU64::new(0),
            embed_errors:     AtomicU64::new(0),
            vision_embed_us:  AtomicU64::new(0),
            text_embed_us:    AtomicU64::new(0),
            embed_total_us:   AtomicU64::new(0),
            queue_depth:      AtomicI64::new(0),
            last_capture_unix: AtomicU64::new(0),
            last_embed_unix:  AtomicU64::new(0),
        }
    }

    /// Snapshot all metrics into a serializable struct.
    pub fn snapshot(&self) -> MetricsSnapshot {
        MetricsSnapshot {
            captures:         self.captures.load(Ordering::Relaxed),
            capture_errors:   self.capture_errors.load(Ordering::Relaxed),
            drops:            self.drops.load(Ordering::Relaxed),
            capture_us:       self.capture_us.load(Ordering::Relaxed),
            ocr_us:           self.ocr_us.load(Ordering::Relaxed),
            resize_us:        self.resize_us.load(Ordering::Relaxed),
            save_us:          self.save_us.load(Ordering::Relaxed),
            capture_total_us: self.capture_total_us.load(Ordering::Relaxed),
            embeds:           self.embeds.load(Ordering::Relaxed),
            embed_errors:     self.embed_errors.load(Ordering::Relaxed),
            vision_embed_us:  self.vision_embed_us.load(Ordering::Relaxed),
            text_embed_us:    self.text_embed_us.load(Ordering::Relaxed),
            embed_total_us:   self.embed_total_us.load(Ordering::Relaxed),
            queue_depth:      self.queue_depth.load(Ordering::Relaxed),
            last_capture_unix: self.last_capture_unix.load(Ordering::Relaxed),
            last_embed_unix:  self.last_embed_unix.load(Ordering::Relaxed),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct MetricsSnapshot {
    pub captures:         u64,
    pub capture_errors:   u64,
    pub drops:            u64,
    pub capture_us:       u64,
    pub ocr_us:           u64,
    pub resize_us:        u64,
    pub save_us:          u64,
    pub capture_total_us: u64,
    pub embeds:           u64,
    pub embed_errors:     u64,
    pub vision_embed_us:  u64,
    pub text_embed_us:    u64,
    pub embed_total_us:   u64,
    pub queue_depth:      i64,
    pub last_capture_unix: u64,
    pub last_embed_unix:  u64,
}

/// Convenience: current time in milliseconds since epoch.
fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as u64
}

// ── Embed job sent from capture thread → embed thread ─────────────────────────

struct EmbedJob {
    row_id:      i64,
    ts_i64:      i64,
    resized_png: Vec<u8>,
    /// Raw capture bytes at full resolution — used for OCR in the embed
    /// thread.  `None` when OCR is disabled to avoid copying large buffers.
    raw_for_ocr: Option<Vec<u8>>,
    config:      ScreenshotConfig,
}

/// Maximum dimension (width or height) for the image passed to the OCR
/// engine.  Full retina captures (5120×2880+) are far too large for text
/// detection and make OCR take 20+ seconds.  Downsizing to 1536px max
/// preserves all readable text while reducing OCR time to <1 second.
const OCR_MAX_DIMENSION: u32 = 1536;

// ── Background worker ─────────────────────────────────────────────────────────

/// Run the screenshot capture worker in a dedicated thread.
/// Called from `lib.rs :: setup_app`.
///
/// Architecture: two threads connected by a bounded channel.
///
/// **Capture thread** (this function) — fast, never blocks on ML:
///   capture → OCR → resize → save WebP → insert SQLite → notify → send job
///
/// **Embed thread** (spawned below) — slow, GPU-bound:
///   receive job → vision embed → HNSW insert → text embed → HNSW insert → UPDATE SQLite
///
/// This ensures the capture cadence is never delayed by slow embedding work
/// and screenshots are always persisted immediately.
pub fn run_screenshot_worker(
    app: AppHandle,
    skill_dir: PathBuf,
    shared_store: Option<Arc<ScreenshotStore>>,
    metrics: Arc<ScreenshotMetrics>,
) {
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

    // ── Spawn the embed thread ──
    // Bounded channel (capacity 4) provides backpressure: if the embed
    // thread falls behind, the capture thread blocks on send rather than
    // accumulating unbounded memory.
    let (embed_tx, embed_rx) = crossbeam_channel::bounded::<EmbedJob>(4);
    let embed_store   = Arc::clone(&store);
    let embed_dir     = skill_dir.clone();
    let embed_app     = app.clone();
    let embed_config  = config.clone();
    let embed_metrics = Arc::clone(&metrics);

    std::thread::Builder::new()
        .name("screenshot-embed".into())
        .spawn(move || {
            run_embed_thread(embed_app, embed_dir, embed_store, embed_rx, embed_config, embed_metrics);
        })
        .expect("[screenshot] failed to spawn embed thread");

    // ── Capture loop ──
    let screenshots_dir = skill_dir.join(SCREENSHOTS_DIR);
    let _ = std::fs::create_dir_all(&screenshots_dir);

    loop {
        // Re-read config + session state in a single lock acquisition
        let (config, session_active) = {
            let r = app.state::<Mutex<Box<AppState>>>();
            let g = r.lock_or_recover();
            let cfg = g.screenshot_config.clone();
            let active = g.session_start_utc.is_some();
            (cfg, active)
        };

        let interval = Duration::from_secs(config.interval_secs.max(1) as u64);
        std::thread::sleep(interval);

        // Gate checks
        if !config.enabled { continue; }
        if config.session_only && !session_active { continue; }

        let iter_start = Instant::now();

        // ── Capture active window ──
        let t0 = Instant::now();
        let captured = match capture_active_window() {
            Some(c) => c,
            None => {
                metrics.capture_errors.fetch_add(1, Ordering::Relaxed);
                continue;
            }
        };
        metrics.capture_us.store(t0.elapsed().as_micros() as u64, Ordering::Relaxed);

        // ── Resize + pad ──
        let t0 = Instant::now();
        let (resized_png, w, h) = match resize_fit_pad(&captured.raw_bytes, config.image_size) {
            Some(r) => r,
            None => continue,
        };
        metrics.resize_us.store(t0.elapsed().as_micros() as u64, Ordering::Relaxed);

        // Keep raw bytes for OCR in embed thread (only if OCR enabled)
        let raw_for_ocr = if config.ocr_enabled {
            Some(captured.raw_bytes.clone())
        } else {
            None
        };
        drop(captured);

        // ── Save to disk as WebP + SQLite + context ──
        let t0 = Instant::now();
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

        let (app_name, window_title) = {
            let r = app.state::<Mutex<Box<AppState>>>();
            let g = r.lock_or_recover();
            match &g.current_active_window {
                Some(aw) => (aw.app_name.clone(), aw.window_title.clone()),
                None => (String::new(), String::new()),
            }
        };

        let ts_i64: i64 = ts_str.parse().unwrap_or(0);

        let row_id = store.insert(&ScreenshotRow {
            timestamp: ts_i64,
            unix_ts,
            filename: webp_name.clone(),
            width: w,
            height: h,
            file_size,
            hnsw_id: None,
            embedding: None,
            embedding_dim: 0,
            model_backend: String::new(),
            model_id: String::new(),
            image_size: config.image_size,
            quality: config.quality,
            app_name,
            window_title,
            ocr_text: String::new(), // backfilled by embed thread after OCR
            ocr_embedding: None,
            ocr_embedding_dim: 0,
            ocr_hnsw_id: None,
        });

        metrics.save_us.store(t0.elapsed().as_micros() as u64, Ordering::Relaxed);

        // ── Notify frontend ──
        let _ = app.emit("screenshot-captured", ScreenshotCapturedEvent {
            ts: ts_str, filename: webp_name,
        });

        // ── Send to embed thread (non-blocking if capacity available) ──
        if let Some(row_id) = row_id {
            match embed_tx.try_send(EmbedJob {
                row_id,
                ts_i64,
                resized_png,
                raw_for_ocr,
                config: config.clone(),
            }) {
                Ok(()) => {
                    metrics.queue_depth.fetch_add(1, Ordering::Relaxed);
                }
                Err(_) => {
                    metrics.drops.fetch_add(1, Ordering::Relaxed);
                }
            }
        }

        metrics.captures.fetch_add(1, Ordering::Relaxed);
        metrics.capture_total_us.store(iter_start.elapsed().as_micros() as u64, Ordering::Relaxed);
        metrics.last_capture_unix.store(now_ms(), Ordering::Relaxed);
    }
}

/// Embedding thread — processes jobs from the capture thread.
/// Runs vision embedding + OCR text embedding on GPU (when available)
/// and backfills results into SQLite + HNSW.
fn run_embed_thread(
    app: AppHandle,
    skill_dir: PathBuf,
    store: Arc<ScreenshotStore>,
    rx: crossbeam_channel::Receiver<EmbedJob>,
    initial_config: ScreenshotConfig,
    metrics: Arc<ScreenshotMetrics>,
) {
    // Load HNSW indexes
    let mut hnsw = load_or_rebuild_hnsw(&skill_dir, &store);
    let mut ocr_hnsw = load_or_rebuild_ocr_hnsw(&skill_dir, &store);

    // Load vision encoder
    let mut fe_encoder = load_fastembed_image(&initial_config, &skill_dir);
    let mut last_backend = initial_config.embed_backend.clone();
    let mut last_model   = initial_config.fastembed_model.clone();

    // Load OCR engine (downloads models on first use)
    let ocr_engine = if initial_config.ocr_enabled {
        let engine = load_ocr_engine(&skill_dir);
        if engine.is_some() {
            eprintln!("[screenshot-embed] OCR engine ({}) loaded", initial_config.ocr_engine);
        } else {
            eprintln!("[screenshot-embed] OCR engine not available");
        }
        engine
    } else {
        eprintln!("[screenshot-embed] OCR disabled by config");
        None
    };

    // Load text embedder for OCR
    let mut text_embedder: Option<fastembed::TextEmbedding> = if initial_config.ocr_enabled {
        let cache = skill_dir.join("fastembed_cache");
        let model = initial_config.ocr_text_model_enum();
        let eps = build_execution_providers(initial_config.use_gpu);
        eprintln!("[screenshot-embed] OCR text model: {} (gpu={})", initial_config.ocr_text_model, initial_config.use_gpu);
        fastembed::TextEmbedding::try_new(
            fastembed::InitOptions::new(model)
                .with_cache_dir(cache)
                .with_execution_providers(eps)
        ).ok()
    } else {
        None
    };

    let mut inserts_since_save: usize = 0;
    let mut ocr_inserts_since_save: usize = 0;

    while let Ok(job) = rx.recv() {
        metrics.queue_depth.fetch_sub(1, Ordering::Relaxed);
        let embed_start = Instant::now();
        let config = &job.config;

        // Hot-reload vision encoder if model changed
        if config.embed_backend != last_backend || config.fastembed_model != last_model {
            eprintln!("[screenshot-embed] model changed — reloading encoder");
            fe_encoder = load_fastembed_image(config, &skill_dir);
            last_backend = config.embed_backend.clone();
            last_model   = config.fastembed_model.clone();
        }

        // ── Vision embedding ──
        let t0 = Instant::now();
        let (embedding, model_backend, model_id) = match config.embed_backend.as_str() {
            "fastembed" => {
                if let Some(ref mut fe) = fe_encoder {
                    let emb = fastembed_embed(fe, &job.resized_png);
                    let mid = config.model_id();
                    (emb, "fastembed".to_string(), mid)
                } else {
                    (None, String::new(), String::new())
                }
            }
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
                            bytes: job.resized_png.clone(),
                            result_tx: tx,
                        }).ok()?;
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
                { (None, String::new(), String::new()) }
            }
            _ => (None, String::new(), String::new()),
        };

        metrics.vision_embed_us.store(t0.elapsed().as_micros() as u64, Ordering::Relaxed);

        // ── Backfill vision embedding ──
        if let Some(ref emb) = embedding {
            let id = hnsw.len() as u64;
            hnsw.insert(emb.clone(), job.ts_i64);
            inserts_since_save += 1;
            if inserts_since_save >= SCREENSHOT_HNSW_SAVE_EVERY {
                save_hnsw(&hnsw, &skill_dir);
                inserts_since_save = 0;
            }
            store.update_embedding(
                job.row_id, emb, Some(id),
                &model_backend, &model_id, config.image_size,
            );
        }

        // ── OCR extraction (on raw full-res image, downsized to OCR_MAX_DIMENSION) ──
        let t_ocr = Instant::now();
        let ocr_text = if let (Some(ref engine), Some(ref raw)) = (&ocr_engine, &job.raw_for_ocr) {
            run_ocr(engine, raw).unwrap_or_default()
        } else {
            String::new()
        };
        metrics.ocr_us.store(t_ocr.elapsed().as_micros() as u64, Ordering::Relaxed);

        // ── OCR text embedding + backfill ──
        let t0 = Instant::now();
        if !ocr_text.is_empty() {
            if let Some(ref mut te) = text_embedder {
                if let Ok(mut vecs) = te.embed(vec![ocr_text.as_str()], None) {
                    if let Some(emb) = vecs.pop() {
                        let id = ocr_hnsw.len() as u64;
                        ocr_hnsw.insert(emb.clone(), job.ts_i64);
                        ocr_inserts_since_save += 1;
                        if ocr_inserts_since_save >= SCREENSHOT_HNSW_SAVE_EVERY {
                            save_ocr_hnsw(&ocr_hnsw, &skill_dir);
                            ocr_inserts_since_save = 0;
                        }
                        store.update_ocr(job.row_id, &ocr_text, Some(&emb), Some(id));
                    }
                }
            } else {
                // No text embedder — still save the OCR text without embedding
                store.update_ocr(job.row_id, &ocr_text, None, None);
            }
        }
        metrics.text_embed_us.store(t0.elapsed().as_micros() as u64, Ordering::Relaxed);

        metrics.embeds.fetch_add(1, Ordering::Relaxed);
        metrics.embed_total_us.store(embed_start.elapsed().as_micros() as u64, Ordering::Relaxed);
        metrics.last_embed_unix.store(now_ms(), Ordering::Relaxed);
    }

    // Channel closed — save indexes before exit
    save_hnsw(&hnsw, &skill_dir);
    save_ocr_hnsw(&ocr_hnsw, &skill_dir);
    eprintln!("[screenshot-embed] thread exiting — indexes saved");
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

/// Search screenshots by OCR text similarity using the OCR HNSW index.
/// Embeds the query text with fastembed, then searches the OCR HNSW.
pub fn search_by_ocr_text_embedding(
    skill_dir: &Path,
    store: &ScreenshotStore,
    query: &str,
    k: usize,
    config: &ScreenshotConfig,
) -> Vec<ScreenshotResult> {
    // Embed the query text
    let query_emb = embed_ocr_text(query, skill_dir, config);
    let query_emb = match query_emb {
        Some(v) => v,
        None => return vec![],
    };

    // Load OCR HNSW
    let hnsw_path = skill_dir.join(SCREENSHOTS_OCR_HNSW);
    let hnsw = match LabeledIndex::<Cosine, i64>::load(&hnsw_path, Cosine) {
        Ok(idx) => idx,
        Err(_) => return vec![],
    };

    search_by_vector(&hnsw, store, &query_emb, k)
}

/// Search screenshots by OCR text substring (SQL LIKE).
pub fn search_by_ocr_text_like(
    store: &ScreenshotStore,
    query: &str,
    limit: usize,
) -> Vec<ScreenshotResult> {
    store.search_by_ocr_text(query, limit)
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
