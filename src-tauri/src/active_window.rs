// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
//! Active-window tracker + input-activity monitor.
//!
//! ## Active-window poller (`run_poller`)
//! Runs in its own OS thread, wakes every second, asks the OS for the
//! frontmost window, and — when the app or title changes — stores the new
//! entry in `activity.sqlite` and emits the `"active-window-changed"` Tauri
//! event.
//!
//! ## Input monitor (`run_input_monitor`)
//! Polls the OS every second for the time since the last keyboard / mouse
//! event using permission-free platform APIs:
//!
//! | Platform | API | Permission |
//! |---|---|---|
//! | macOS | `CGEventSourceSecondsSinceLastEventType` | None |
//! | Linux | `xprintidle` command | None (needs `xprintidle` installed) |
//! | Windows | `GetLastInputInfo` (user32) | None |
//!
//! A 1 Hz polling loop records which seconds had keyboard or mouse activity,
//! updates `AtomicU64` timestamps, and every 60 s flushes a bucket row to
//! `activity.sqlite` with the count of active seconds in that minute.

use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

use crate::{AppState, MutexExt};
use skill_data::activity_store::ActivityStore;

// Re-export the shared data type from skill-data so existing `crate::active_window::ActiveWindowInfo`
// imports keep working throughout the Tauri app.
pub use skill_data::active_window::ActiveWindowInfo;

// ── Platform OS queries ───────────────────────────────────────────────────────

/// Query the OS for the currently focused window.
/// Returns `None` when the query fails or the tool is absent.
#[cfg(target_os = "macos")]
pub fn poll_active_window() -> Option<ActiveWindowInfo> {
    let script = r#"
tell application "System Events"
    set frontApp to first application process whose frontmost is true
    set appName to name of frontApp
    try
        set appPath to POSIX path of (application file of frontApp)
    on error
        set appPath to ""
    end try
    try
        set winTitle to name of front window of frontApp
    on error
        set winTitle to ""
    end try
    return appName & "|||" & appPath & "|||" & winTitle
end tell"#;

    let out = std::process::Command::new("osascript")
        .args(["-e", script])
        .output()
        .ok()?;
    if !out.status.success() { return None; }

    let raw = String::from_utf8_lossy(&out.stdout);
    let raw = raw.trim();
    let mut parts = raw.splitn(3, "|||");
    let app_name     = parts.next().unwrap_or("").trim().to_string();
    let app_path     = parts.next().unwrap_or("").trim().to_string();
    let window_title = parts.next().unwrap_or("").trim().to_string();
    if app_name.is_empty() { return None; }

    Some(ActiveWindowInfo { app_name, app_path, window_title, activated_at: crate::unix_secs() })
}

#[cfg(target_os = "linux")]
pub fn poll_active_window() -> Option<ActiveWindowInfo> {
    let win_id_out = std::process::Command::new("xdotool")
        .arg("getactivewindow")
        .output()
        .ok()
        .filter(|o| o.status.success())?;
    let win_id = String::from_utf8_lossy(&win_id_out.stdout).trim().to_string();
    if win_id.is_empty() { return None; }

    let window_title = std::process::Command::new("xdotool")
        .args(["getwindowname", &win_id])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();

    let wm_class = std::process::Command::new("xprop")
        .args(["-id", &win_id, "WM_CLASS"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();

    let app_name = wm_class
        .split('"')
        .nth(3)
        .map(std::string::ToString::to_string)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| window_title.clone());

    let pid_prop = std::process::Command::new("xprop")
        .args(["-id", &win_id, "_NET_WM_PID"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();

    let app_path = pid_prop
        .split('=')
        .nth(1)
        .and_then(|s| s.trim().parse::<u32>().ok())
        .and_then(|pid| std::fs::read_link(format!("/proc/{pid}/exe")).ok())
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();

    Some(ActiveWindowInfo { app_name, app_path, window_title, activated_at: crate::unix_secs() })
}

#[cfg(target_os = "windows")]
pub fn poll_active_window() -> Option<ActiveWindowInfo> {
    // ── Pure Win32 FFI — no PowerShell, no subprocess, no .NET JIT ──────────
    //
    // The original implementation spawned `powershell -Command "Add-Type …"`
    // every second.  PowerShell startup (~300 ms) plus C# JIT via `Add-Type`
    // made this visibly expensive: Task Manager showed a new powershell.exe
    // process every second, the poller stalled for ~500 ms per tick, and the
    // constant process-creation noise broke the UX.
    //
    // Replacement: call the same Win32 APIs directly from Rust via FFI.
    // Every function used here has been available since Windows XP/Vista and
    // requires no elevated privileges.
    //
    //  user32  → GetForegroundWindow, GetWindowTextW, GetWindowThreadProcessId
    //  kernel32 → OpenProcess, QueryFullProcessImageNameW, CloseHandle

    type Hwnd    = *mut core::ffi::c_void;
    type Handle  = *mut core::ffi::c_void;
    type Dword   = u32;
    type Bool    = i32;
    type Wchar   = u16;

    // PROCESS_QUERY_LIMITED_INFORMATION — sufficient for QueryFullProcessImageNameW
    // and available without elevation for most processes (Vista+).
    const PROCESS_QUERY_LIMITED_INFORMATION: Dword = 0x1000;

    #[link(name = "user32")]
    extern "system" {
        fn GetForegroundWindow() -> Hwnd;
        fn GetWindowTextW(hwnd: Hwnd, lp_string: *mut Wchar, n_max_count: i32) -> i32;
        fn GetWindowThreadProcessId(hwnd: Hwnd, lpdw_process_id: *mut Dword) -> Dword;
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn OpenProcess(
            dw_desired_access: Dword,
            b_inherit_handle:  Bool,
            dw_process_id:     Dword,
        ) -> Handle;
        fn QueryFullProcessImageNameW(
            h_process:   Handle,
            dw_flags:    Dword,
            lp_exe_name: *mut Wchar,
            lpdw_size:   *mut Dword,
        ) -> Bool;
        fn CloseHandle(h_object: Handle) -> Bool;
    }

    // SAFETY: All Win32 FFI calls below use valid handles obtained from the OS.
    // Buffers are stack-allocated with known sizes and passed with correct lengths.
    // Handles are closed after use. No aliasing or lifetime issues.
    unsafe {
        // 1. Foreground window handle.
        let hwnd = GetForegroundWindow();
        if hwnd.is_null() { return None; }

        // 2. Window title (wide string).
        let mut title_buf = [0u16; 512];
        let title_len = GetWindowTextW(hwnd, title_buf.as_mut_ptr(), title_buf.len() as i32);
        let window_title = if title_len > 0 {
            String::from_utf16_lossy(&title_buf[..title_len as usize])
        } else {
            String::new()
        };

        // 3. Process ID owning the foreground window.
        let mut pid: Dword = 0;
        GetWindowThreadProcessId(hwnd, &mut pid);
        if pid == 0 { return None; }

        // 4. Open the process with minimal rights to read its image path.
        let hproc = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        let (app_path, app_name) = if !hproc.is_null() {
            let mut path_buf = [0u16; 1024];
            let mut path_len = path_buf.len() as Dword;
            let app_path = if QueryFullProcessImageNameW(
                hproc, 0, path_buf.as_mut_ptr(), &mut path_len,
            ) != 0 && path_len > 0 {
                String::from_utf16_lossy(&path_buf[..path_len as usize])
            } else {
                String::new()
            };
            CloseHandle(hproc);

            // Derive a human-readable name from the exe filename (no extension).
            let app_name = std::path::Path::new(&app_path)
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            (app_path, app_name)
        } else {
            (String::new(), String::new())
        };

        if app_name.is_empty() && window_title.is_empty() { return None; }

        Some(ActiveWindowInfo {
            app_name,
            app_path,
            window_title,
            activated_at: crate::unix_secs(),
        })
    }
}

// ── Platform input-idle detection ─────────────────────────────────────────────

/// Returns `(kbd_active, mouse_active)` — whether each device had an event
/// within the last `ACTIVE_THRESHOLD_SECS` seconds.
///
/// Uses permission-free platform APIs; no Accessibility / XRecord / hooks.
const ACTIVE_THRESHOLD_SECS: f64 = crate::constants::ACTIVE_WINDOW_IDLE_THRESHOLD_SECS;

#[cfg(target_os = "macos")]
fn poll_input_activity() -> (bool, bool) {
    // CoreGraphics idle-time query — available without any OS permission.
    // kCGEventSourceStateCombinedSessionState = 1
    type CGEventSourceStateID = i32;
    type CGEventType = u32;

    // Event type constants from <CoreGraphics/CGEventTypes.h>
    const STATE:              CGEventSourceStateID = 1;
    const KEY_DOWN:           CGEventType = 10;
    const MOUSE_MOVED:        CGEventType = 5;
    const LEFT_MOUSE_DOWN:    CGEventType = 1;
    const RIGHT_MOUSE_DOWN:   CGEventType = 3;
    const SCROLL_WHEEL:       CGEventType = 22;
    const OTHER_MOUSE_DOWN:   CGEventType = 25;

    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGEventSourceSecondsSinceLastEventType(
            state:     CGEventSourceStateID,
            event_type: CGEventType,
        ) -> f64;
    }

    // SAFETY: CGEventSourceSecondsSinceLastEventType is a thread-safe
    // CoreGraphics query that only reads system event timestamps.
    unsafe {
        let kbd_idle = CGEventSourceSecondsSinceLastEventType(STATE, KEY_DOWN);
        // Take the minimum idle time across the mouse event types we care about.
        let mouse_idle = [MOUSE_MOVED, LEFT_MOUSE_DOWN, RIGHT_MOUSE_DOWN,
                          SCROLL_WHEEL, OTHER_MOUSE_DOWN]
            .iter()
            .map(|&ty| CGEventSourceSecondsSinceLastEventType(STATE, ty))
            .fold(f64::INFINITY, f64::min);

        (kbd_idle < ACTIVE_THRESHOLD_SECS, mouse_idle < ACTIVE_THRESHOLD_SECS)
    }
}

#[cfg(target_os = "linux")]
fn poll_input_activity() -> (bool, bool) {
    // `xprintidle` prints milliseconds since the last X11 input event.
    // Install: sudo apt install xprintidle  (or equivalent)
    use std::sync::atomic::AtomicBool;
    static MISSING: AtomicBool = AtomicBool::new(false);

    if MISSING.load(Ordering::Relaxed) {
        return (false, false);
    }

    let in_wayland = std::env::var("XDG_SESSION_TYPE")
        .map(|v| v.eq_ignore_ascii_case("wayland"))
        .unwrap_or(false)
        || std::env::var("WAYLAND_DISPLAY")
            .map(|v| !v.trim().is_empty())
            .unwrap_or(false);

    if in_wayland {
        MISSING.store(true, Ordering::Relaxed);
        eprintln!(
            "[input-monitor] Wayland session detected — xprintidle is X11-only; keyboard/mouse idle tracking unavailable"
        );
        return (false, false);
    }

    let out = match std::process::Command::new("xprintidle").output() {
        Ok(o) if o.status.success() => o,
        _ => {
            MISSING.store(true, Ordering::Relaxed);
            eprintln!(
                "[input-monitor] xprintidle not found — keyboard/mouse idle \
                 tracking unavailable. Install with: sudo apt install xprintidle"
            );
            return (false, false);
        }
    };

    let ms: f64 = String::from_utf8_lossy(&out.stdout)
        .trim()
        .parse()
        .unwrap_or(f64::MAX);
    // xprintidle doesn't distinguish keyboard vs mouse — report both.
    let active = ms < (ACTIVE_THRESHOLD_SECS * 1_000.0);
    (active, active)
}

#[cfg(target_os = "windows")]
fn poll_input_activity() -> (bool, bool) {
    use std::mem;

    #[repr(C)]
    struct Lastinputinfo {
        cb_size: u32,
        dw_time: u32,
    }

    #[link(name = "user32")]
    extern "system" {
        fn GetLastInputInfo(plii: *mut Lastinputinfo) -> i32;
        fn GetTickCount() -> u32;
    }

    // SAFETY: `Lastinputinfo` is a simple repr(C) struct initialised with the correct
    // `cb_size`. `GetLastInputInfo` and `GetTickCount` are safe Win32 calls with no
    // aliasing or lifetime concerns.
    unsafe {
        let mut info = Lastinputinfo {
            cb_size: mem::size_of::<Lastinputinfo>() as u32,
            dw_time: 0,
        };
        if GetLastInputInfo(&mut info) == 0 {
            return (false, false);
        }
        let now_tick = GetTickCount();
        let idle_ms  = now_tick.wrapping_sub(info.dw_time) as f64;
        // GetLastInputInfo doesn't distinguish keyboard vs mouse.
        let active = idle_ms < (ACTIVE_THRESHOLD_SECS * 1_000.0);
        (active, active)
    }
}

// ── Active-window poller ──────────────────────────────────────────────────────

/// Runs forever in a dedicated thread.  Wakes every second, queries the OS
/// for the active window, writes new entries to `activity.sqlite`, and emits
/// `"active-window-changed"` when the app or title changes.
pub fn run_poller(app: AppHandle, store: Arc<ActivityStore>) {
    let mut last: Option<ActiveWindowInfo> = None;

    loop {
        std::thread::sleep(Duration::from_secs(1));

        let enabled = app
            .state::<std::sync::Mutex<Box<AppState>>>()
            .lock_or_recover()
            .input.track_active_window;

        if !enabled {
            if last.is_some() {
                last = None;
                app.state::<std::sync::Mutex<Box<AppState>>>()
                    .lock_or_recover()
                    .input.current_active_window = None;
                let _ = app.emit("active-window-changed", Option::<ActiveWindowInfo>::None);
            }
            continue;
        }

        let current = poll_active_window();

        let changed = match (&last, &current) {
            (None, None)         => false,
            (None, Some(_))      => true,
            (Some(_), None)      => true,
            (Some(prev), Some(cur)) =>
                prev.app_name != cur.app_name || prev.window_title != cur.window_title,
        };

        if changed {
            if let Some(info) = &current {
                store.insert_active_window(info);
                app.state::<std::sync::Mutex<Box<AppState>>>()
                    .lock_or_recover()
                    .input.current_active_window = current.clone();
                let _ = app.emit("active-window-changed", info.clone());
            } else {
                app.state::<std::sync::Mutex<Box<AppState>>>()
                    .lock_or_recover()
                    .input.current_active_window = None;
                let _ = app.emit("active-window-changed", Option::<ActiveWindowInfo>::None);
            }
            last = current;
        }
    }
}

// ── Input monitor ─────────────────────────────────────────────────────────────

/// Polls the OS every second for keyboard / mouse activity and stores the
/// results in `activity.sqlite`.
///
/// ## How it works
/// Each second the loop calls `poll_input_activity()` which asks the OS
/// "how long ago was the last keyboard / mouse event?" via a permission-free
/// system API (CGEventSource on macOS, `xprintidle` on Linux, `GetLastInputInfo`
/// on Windows).  If the answer is under two seconds the device is considered
/// active in that second — the relevant `AtomicU64` timestamp is updated and
/// a count is incremented.
///
/// Every 60 s the accumulated counts are written to the `input_buckets` table
/// as a per-minute row, suitable for charting activity over time.
///
/// The `enabled_flag` `AtomicBool` is checked on every iteration and can be
/// flipped by `set_input_activity_tracking` without a restart.
pub fn run_input_monitor(
    app:          AppHandle,
    enabled_flag: Arc<AtomicBool>,
    kbd_ts:       Arc<AtomicU64>,
    mouse_ts:     Arc<AtomicU64>,
    kbd_count:    Arc<AtomicU64>,
    mouse_count:  Arc<AtomicU64>,
    store:        Arc<ActivityStore>,
) {
    // Tracks the previous emitted timestamps so we only call app.emit on change.
    let mut prev_emit_kbd:   u64 = 0;
    let mut prev_emit_mouse: u64 = 0;

    // Tracks the count snapshots at the last 60-s flush.
    let mut prev_flush_kbd:   u64 = 0;
    let mut prev_flush_mouse: u64 = 0;
    let mut last_flush_at:    u64 = 0;

    loop {
        std::thread::sleep(Duration::from_secs(1));

        if !enabled_flag.load(Ordering::Relaxed) {
            continue;
        }

        let now = crate::unix_secs();

        // ── Poll platform input-idle API ──────────────────────────────────────
        let (kbd_active, mouse_active) = poll_input_activity();

        if kbd_active {
            kbd_ts.store(now, Ordering::Relaxed);
            kbd_count.fetch_add(1, Ordering::Relaxed);
        }
        if mouse_active {
            mouse_ts.store(now, Ordering::Relaxed);
            mouse_count.fetch_add(1, Ordering::Relaxed);
        }

        // ── Emit UI update on change ──────────────────────────────────────────
        let k = kbd_ts.load(Ordering::Relaxed);
        let m = mouse_ts.load(Ordering::Relaxed);
        if k != prev_emit_kbd || m != prev_emit_mouse {
            prev_emit_kbd   = k;
            prev_emit_mouse = m;
            let _ = app.emit("input-activity", (k, m));
        }

        // ── 60-second DB flush ────────────────────────────────────────────────
        if now >= last_flush_at + 60 {
            last_flush_at = now;

            // Legacy last-seen-timestamp row.
            if k > 0 || m > 0 {
                store.insert_input_activity(
                    if k > 0 { Some(k) } else { None },
                    if m > 0 { Some(m) } else { None },
                    now,
                );
            }

            // Per-minute event-count bucket for charts.
            let kc = kbd_count.load(Ordering::Relaxed);
            let mc = mouse_count.load(Ordering::Relaxed);
            let dk = kc.saturating_sub(prev_flush_kbd);
            let dm = mc.saturating_sub(prev_flush_mouse);
            prev_flush_kbd   = kc;
            prev_flush_mouse = mc;

            if dk > 0 || dm > 0 {
                // Assign the counts to the current calendar minute.
                let bucket_ts = now / 60 * 60;
                store.upsert_input_bucket(bucket_ts, dk, dm);
            }
        }
    }
}
