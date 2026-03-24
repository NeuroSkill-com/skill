// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Platform-specific window capture (macOS, Linux, Windows).

#[cfg(target_os = "macos")]
use std::io::Cursor;
#[cfg(target_os = "macos")]
use std::path::Path;

#[cfg(target_os = "macos")]
use image::{GenericImageView, ImageReader};

use std::time::Duration;

// ── Captured image ────────────────────────────────────────────────────────────

#[allow(dead_code)]
pub(crate) struct CapturedImage {
    /// Raw encoded bytes (PNG on macOS from screencapture).
    /// On Linux/Windows with xcap, this is empty — use `decoded` instead.
    pub(crate) raw_bytes: Vec<u8>,
    /// Pre-decoded image (avoids encode→decode round-trip on Linux/Windows).
    pub(crate) decoded: Option<image::DynamicImage>,
    pub(crate) width:     u32,
    pub(crate) height:    u32,
}

// ── Motion detection ──────────────────────────────────────────────────────────

/// Compare two images (PNG/WebP bytes) and return the fraction of pixels that
/// differ beyond a per-channel tolerance.  Used to detect animation/scrolling.
///
/// Both images should be the same dimensions (e.g. already resized to the
/// target capture size).  Returns `1.0` if dimensions differ, `0.0` on error.
#[allow(dead_code)]
pub(crate) fn motion_score(prev: &[u8], curr: &[u8]) -> f32 {
    let prev_img = match image::load_from_memory(prev) {
        Ok(img) => img.to_rgba8(),
        Err(_) => return 0.0,
    };
    let curr_img = match image::load_from_memory(curr) {
        Ok(img) => img.to_rgba8(),
        Err(_) => return 0.0,
    };

    if prev_img.dimensions() != curr_img.dimensions() {
        return 1.0;
    }

    let total_pixels = prev_img.width() as usize * prev_img.height() as usize;
    if total_pixels == 0 {
        return 0.0;
    }

    // Per-channel noise tolerance — ignores compression artefacts and
    // minor rendering differences (cursor blink, subpixel AA changes).
    const TOLERANCE: u8 = 12;

    let changed = prev_img
        .pixels()
        .zip(curr_img.pixels())
        .filter(|(p, c)| {
            p[0].abs_diff(c[0]) > TOLERANCE
                || p[1].abs_diff(c[1]) > TOLERANCE
                || p[2].abs_diff(c[2]) > TOLERANCE
        })
        .count();

    changed as f32 / total_pixels as f32
}

// ── Burst capture ─────────────────────────────────────────────────────────────

/// Capture `count` frames in rapid succession with `delay` between each.
/// Skips frames where the platform capture fails.
/// Currently only used by the script-level GIF API, not the periodic capture loop.
#[allow(dead_code)]
pub(crate) fn capture_burst(count: u32, delay: Duration) -> Vec<CapturedImage> {
    let mut frames = Vec::with_capacity(count as usize);
    for _ in 0..count {
        if let Some(img) = capture_active_window() {
            frames.push(img);
        }
        std::thread::sleep(delay);
    }
    frames
}

// ── Platform window capture ───────────────────────────────────────────────────

/// Capture the active application window.
/// Returns `None` if capture fails or is unsupported.
pub(crate) fn capture_active_window() -> Option<CapturedImage> {
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
fn read_captured_image(path: &Path) -> Option<CapturedImage> {
    let raw_bytes = std::fs::read(path).ok()?;
    let _ = std::fs::remove_file(path);
    if raw_bytes.is_empty() { return None; }
    let img = ImageReader::new(Cursor::new(&raw_bytes))
        .with_guessed_format().ok()?
        .decode().ok()?;
    let (w, h) = img.dimensions();
    Some(CapturedImage { raw_bytes, decoded: Some(img), width: w, height: h })
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
    ///
    /// SAFETY: `s` must be a NUL-terminated byte slice. The returned CFString
    /// must be released by the caller via `CFRelease`.
    unsafe fn cfstr(s: &[u8]) -> CFStringRef {
        // SAFETY: `s` points to a static NUL-terminated literal.
        unsafe { CFStringCreateWithCString(std::ptr::null(), s.as_ptr() as *const c_char, K_CF_STRING_ENCODING_UTF8) }
    }

    /// Helper: get an i32 from a CFNumber.
    ///
    /// SAFETY: `n` must be a valid CFNumber (or null, which is handled).
    unsafe fn cfnum_i32(n: CFTypeRef) -> Option<i32> {
        if n.is_null() { return None; }
        let mut v: i64 = 0;
        // SAFETY: `n` is a non-null CFNumber; `v` is properly sized.
        if unsafe { CFNumberGetValue(n, K_CF_NUMBER_SINT32_TYPE, &mut v) } {
            Some(v as i32)
        } else { None }
    }

    /// Helper: get an i64 from a CFNumber (some fields may be i64).
    ///
    /// SAFETY: `n` must be a valid CFNumber (or null, which is handled).
    unsafe fn cfnum_i64(n: CFTypeRef) -> Option<i64> {
        if n.is_null() { return None; }
        let mut v: i64 = 0;
        // SAFETY: `n` is a non-null CFNumber; `v` is properly sized for both
        // the 64-bit and 32-bit read attempts.
        if unsafe { CFNumberGetValue(n, K_CF_NUMBER_SINT64_TYPE, &mut v) } {
            Some(v)
        } else {
            if unsafe { CFNumberGetValue(n, K_CF_NUMBER_SINT32_TYPE, &mut v) } {
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
        // SAFETY: NSWorkspace and NSRunningApplication are stable AppKit APIs.
        // `frontmostApplication` returns a nullable NSRunningApplication.
        // `processIdentifier` returns a pid_t (i32).
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
    #[cfg(feature = "capture")]
    { capture_xcap() }
    #[cfg(not(feature = "capture"))]
    {
        eprintln!("[screenshot] capture disabled (built without `capture` feature / xcap)");
        None
    }
}

/// Capture via the `xcap` crate — works on both X11 and Wayland without
/// requiring external tools (grim, scrot, xdotool, etc.).
#[cfg(all(target_os = "linux", feature = "capture"))]
fn capture_xcap() -> Option<CapturedImage> {
    // ── Attempt 1: capture the focused window ──
    if let Ok(windows) = xcap::Window::all() {
        // Find the currently-focused (frontmost) window.
        // xcap returns is_minimized(); we want the first non-minimized window
        // that reports as the current/focused window.
        for win in &windows {
            let dominated = win.is_minimized().unwrap_or(true);
            if dominated { continue; }
            // xcap on Linux/Wayland: `current_monitor` + first non-minimized
            // window with a title is a reasonable heuristic.  The list is
            // ordered front-to-back on most compositors.
            let title = win.title().unwrap_or_default();
            if title.is_empty() { continue; }

            match win.capture_image() {
                Ok(rgba) => {
                    let w = rgba.width();
                    let h = rgba.height();
                    if w == 0 || h == 0 { continue; }

                    // Check for dark/empty screenshot (all zeros or near-zero)
                    let sample = rgba.as_raw();
                    let non_zero = sample.iter().take(4096).any(|&b| b > 5);
                    if !non_zero { continue; }

                    // Keep decoded image directly — no PNG encode round-trip
                    let dyn_img = image::DynamicImage::ImageRgba8(rgba);
                    return Some(CapturedImage { raw_bytes: Vec::new(), decoded: Some(dyn_img), width: w, height: h });
                }
                Err(e) => {
                    eprintln!("[screenshot] xcap window capture failed ({}): {e}", title);
                    continue;
                }
            }
        }
    }

    // ── Attempt 2: full-screen capture of the primary monitor ──
    capture_xcap_monitor()
}

/// Full-screen capture of the primary monitor via xcap.
/// Shared fallback for both Linux and Windows capture paths.
#[cfg(all(any(target_os = "linux", target_os = "windows"), feature = "capture"))]
fn capture_xcap_monitor() -> Option<CapturedImage> {
    if let Ok(monitors) = xcap::Monitor::all() {
        // Prefer the primary monitor, fall back to the first one.
        let monitor = monitors.iter().find(|m| m.is_primary().unwrap_or(false)).or_else(|| monitors.first());
        if let Some(mon) = monitor {
            match mon.capture_image() {
                Ok(rgba) => {
                    let w = rgba.width();
                    let h = rgba.height();
                    if w == 0 || h == 0 { return None; }
                    let dyn_img = image::DynamicImage::ImageRgba8(rgba);
                    return Some(CapturedImage { raw_bytes: Vec::new(), decoded: Some(dyn_img), width: w, height: h });
                }
                Err(e) => {
                    eprintln!("[screenshot] xcap monitor capture failed: {e}");
                }
            }
        }
    }
    None
}

/// Windows-specific capture — targets only the foreground window.
///
/// The generic Linux `capture_xcap()` iterates through ALL non-minimized
/// windows and calls `capture_image()` on each one until it gets a result.
/// On Windows, xcap's GDI capture uses `PrintWindow` which sends a
/// `WM_PRINT` message to the target window, forcing it to repaint.  When
/// this is called on multiple windows every few seconds, it causes constant
/// visible flickering across all open windows.
///
/// This implementation uses xcap's `is_focused()` to find the single active
/// foreground window, then captures only that one — no iteration, no
/// spurious `PrintWindow` calls on background windows.
#[cfg(target_os = "windows")]
fn capture_windows() -> Option<CapturedImage> {
    #[cfg(feature = "capture")]
    {
        capture_windows_foreground()
            .or_else(capture_xcap_monitor)
    }
    #[cfg(not(feature = "capture"))]
    {
        eprintln!("[screenshot] capture disabled (built without `capture` feature / xcap)");
        None
    }
}

/// Capture only the foreground window on Windows.
///
/// Enumerates all windows via xcap but uses `is_focused()` to identify
/// the single active foreground window, then captures only that one.
/// This avoids calling `PrintWindow` (via `capture_image()`) on multiple
/// background windows which would cause them to repaint and flicker.
#[cfg(all(target_os = "windows", feature = "capture"))]
fn capture_windows_foreground() -> Option<CapturedImage> {
    let windows = xcap::Window::all().ok()?;

    // Find the focused (foreground) window.
    // xcap's is_focused() calls GetForegroundWindow() internally and
    // compares it to the window's HWND — no capture or WM_PRINT is sent.
    let target = windows.iter().find(|w| {
        w.is_focused().unwrap_or(false)
    });

    if let Some(win) = target {
        if win.is_minimized().unwrap_or(false) {
            return None;
        }

        match win.capture_image() {
            Ok(rgba) => {
                let w = rgba.width();
                let h = rgba.height();
                if w == 0 || h == 0 { return None; }

                // Check for dark/empty screenshot (all zeros or near-zero)
                let sample = rgba.as_raw();
                let non_zero = sample.iter().take(4096).any(|&b| b > 5);
                if !non_zero { return None; }

                let dyn_img = image::DynamicImage::ImageRgba8(rgba);
                return Some(CapturedImage {
                    raw_bytes: Vec::new(),
                    decoded: Some(dyn_img),
                    width: w,
                    height: h,
                });
            }
            Err(e) => {
                eprintln!("[screenshot] xcap foreground window capture failed: {e}");
            }
        }
    }

    None
}

