// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Platform-specific window capture (macOS, Linux, Windows).

use std::io::Cursor;
#[cfg(target_os = "macos")]
use std::path::Path;

use image::{GenericImageView, ImageReader};

// ── Captured image ────────────────────────────────────────────────────────────

#[allow(dead_code)]
pub(crate) struct CapturedImage {
    pub(crate) raw_bytes: Vec<u8>,
    pub(crate) width:     u32,
    pub(crate) height:    u32,
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

