// SPDX-License-Identifier: GPL-3.0-only
//! Daemon-owned active-window and input-activity workers.

use std::{
    sync::{atomic::Ordering, Arc},
    time::Duration,
};

use regex::Regex;
use skill_data::{
    active_window::{ActiveWindowInfo, SecondaryWindowInfo},
    activity_store::ActivityStore,
};
use skill_settings::FilePatternRule;

use crate::state::AppState;

const ACTIVE_THRESHOLD_SECS: f64 = 2.0;

pub fn start_workers(state: AppState) {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let Some(store) = ActivityStore::open(&skill_dir).map(Arc::new) else {
        tracing::warn!("activity store unavailable; activity workers disabled");
        return;
    };

    spawn_resilient("daemon-active-window-poll", state.clone(), store.clone(), run_poller);
    spawn_resilient("daemon-input-monitor", state.clone(), store.clone(), run_input_monitor);
    spawn_resilient("daemon-file-watcher", state.clone(), store.clone(), run_file_watcher);
    spawn_resilient(
        "daemon-eeg-timeseries",
        state.clone(),
        store.clone(),
        run_eeg_timeseries,
    );

    #[cfg(target_os = "macos")]
    spawn_resilient(
        "daemon-clipboard-monitor",
        state.clone(),
        store.clone(),
        run_clipboard_monitor,
    );

    spawn_resilient(
        "daemon-user-screenshot-watcher",
        state,
        store,
        run_user_screenshot_watcher,
    );
}

/// Spawn a named worker thread that automatically restarts on panic.
/// Clones state and store for each attempt so the worker can be retried.
fn spawn_resilient(
    name: &'static str,
    state: AppState,
    store: Arc<ActivityStore>,
    worker: fn(AppState, Arc<ActivityStore>),
) {
    std::thread::Builder::new()
        .name(name.into())
        .spawn(move || loop {
            let s = state.clone();
            let st = store.clone();
            match std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || worker(s, st))) {
                Ok(()) => break, // normal exit
                Err(e) => {
                    tracing::error!("[{name}] worker panicked: {e:?} — restarting in 5s");
                    std::thread::sleep(std::time::Duration::from_secs(5));
                }
            }
        })
        .unwrap_or_else(|_| panic!("failed to spawn {name}"));
}

/// Run osascript with a 3-second timeout. Returns stdout on success, None on
/// timeout or failure. Prevents a hung app from blocking the caller indefinitely.
#[cfg(target_os = "macos")]
fn run_osascript(script: &str) -> Option<String> {
    let mut child = std::process::Command::new("osascript")
        .args(["-e", script])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .ok()?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                if !status.success() {
                    return None;
                }
                break;
            }
            Ok(None) => {
                if std::time::Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return None;
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            Err(_) => return None,
        }
    }
    let out = child.wait_with_output().ok()?;
    Some(String::from_utf8_lossy(&out.stdout).to_string())
}

#[cfg(target_os = "macos")]
fn poll_active_window() -> Option<ActiveWindowInfo> {
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
end tell

-- Try to get the document path from the frontmost application.
-- Only attempt for apps known to be scriptable to avoid the
-- "Choose Application" dialog that macOS shows when osascript
-- cannot resolve a dynamic application name.
set scriptableApps to {"Preview", "TextEdit", "Pages", "Numbers", "Keynote", "Xcode", "Script Editor", "Finder"}
set docPath to ""
if scriptableApps contains appName then
    try
        tell application appName
            try
                set docPath to POSIX path of (file of front document as alias)
            on error
                try
                    set docPath to POSIX path of (path of front document)
                on error
                    set docPath to ""
                end try
            end try
        end tell
    on error
        set docPath to ""
    end try
end if

return appName & "|||" & appPath & "|||" & winTitle & "|||" & docPath"#;

    let raw = run_osascript(script)?;
    let raw = raw.trim();
    let mut parts = raw.splitn(4, "|||");
    let app_name = parts.next().unwrap_or("").trim().to_string();
    let app_path = parts.next().unwrap_or("").trim().to_string();
    let window_title = parts.next().unwrap_or("").trim().to_string();
    let document_path = parts.next().unwrap_or("").trim().to_string();
    if app_name.is_empty() {
        return None;
    }

    Some(ActiveWindowInfo {
        app_name,
        app_path,
        window_title,
        document_path: if document_path.is_empty() {
            None
        } else {
            Some(document_path)
        },
        activated_at: unix_secs(),
        browser_title: None, // Enriched later in run_poller.
        monitor_id: None,    // Enriched later if multi-monitor detection succeeds.
    })
}

/// Poll all visible windows on non-primary monitors (macOS only).
/// Returns a list of windows that are on secondary screens.
#[cfg(target_os = "macos")]
fn poll_secondary_windows() -> Vec<SecondaryWindowInfo> {
    // Use AppleScript to get all visible windows with their positions,
    // then compare against screen bounds to determine which monitor.
    let script = r#"
set result to ""
tell application "System Events"
    set frontName to name of first application process whose frontmost is true
    repeat with proc in (application processes whose visible is true)
        set procName to name of proc
        if procName is not frontName then
            try
                repeat with w in windows of proc
                    try
                        set winTitle to name of w
                        set winPos to position of w
                        set xPos to item 1 of winPos
                        -- Use x position to infer monitor (primary is typically x >= 0 and < primary width)
                        set result to result & procName & "|||" & winTitle & "|||" & xPos & linefeed
                    end try
                end repeat
            end try
        end if
    end repeat
end tell
return result"#;

    let out = match run_osascript(script) {
        Some(s) => s,
        None => return vec![],
    };

    // Parse: each line is "appName|||windowTitle|||xPosition"
    // Query actual primary screen width to avoid hardcoded values.
    let primary_width: i64 = run_osascript("tell application \"Finder\" to get bounds of window of desktop")
        .and_then(|s| s.split(',').nth(2)?.trim().parse::<i64>().ok())
        .unwrap_or(2000);

    out.lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() {
                return None;
            }
            let mut parts = line.splitn(3, "|||");
            let app_name = parts.next()?.trim().to_string();
            let window_title = parts.next()?.trim().to_string();
            let x_pos: i64 = parts.next()?.trim().parse().ok()?;
            if app_name.is_empty() || window_title.is_empty() {
                return None;
            }
            // If window is outside primary monitor bounds, it's on a secondary monitor.
            if x_pos < 0 || x_pos >= primary_width {
                Some(SecondaryWindowInfo {
                    app_name,
                    window_title,
                    monitor_id: if x_pos < 0 { 2 } else { 1 },
                })
            } else {
                None
            }
        })
        .collect()
}

/// Poll visible windows on non-primary monitors (Linux).
/// Uses `wmctrl -lG` which lists all windows with geometry (x, y, w, h).
#[cfg(target_os = "linux")]
fn poll_secondary_windows() -> Vec<SecondaryWindowInfo> {
    let out = match std::process::Command::new("wmctrl").args(["-lG"]).output() {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).to_string(),
        _ => return vec![],
    };

    // Get the active window ID to exclude it.
    let active_id = std::process::Command::new("xdotool")
        .arg("getactivewindow")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();

    // Query primary monitor width from xrandr.
    let primary_width: i64 = std::process::Command::new("xrandr")
        .arg("--current")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| {
            // Find line with " primary " and parse resolution like "1920x1080+0+0".
            let s = String::from_utf8_lossy(&o.stdout);
            s.lines().find(|l| l.contains(" primary ")).and_then(|l| {
                l.split_whitespace()
                    .find(|w| w.contains('x') && w.contains('+'))
                    .and_then(|res| res.split('x').next()?.parse::<i64>().ok())
            })
        })
        .unwrap_or(2000);

    out.lines()
        .filter_map(|line| {
            // wmctrl -lG format: 0x04000007  0 100 200 800 600 hostname Window Title
            let parts: Vec<&str> = line.splitn(8, char::is_whitespace).collect();
            if parts.len() < 8 {
                return None;
            }
            let win_id = parts[0].trim();
            // Skip the active window.
            if !active_id.is_empty() {
                // xdotool returns decimal, wmctrl returns hex — compare carefully.
                if let Ok(active_dec) = active_id.parse::<u64>() {
                    if let Ok(this_hex) = u64::from_str_radix(win_id.trim_start_matches("0x"), 16) {
                        if active_dec == this_hex {
                            return None;
                        }
                    }
                }
            }
            let x: i64 = parts[2].trim().parse().ok()?;
            let title = parts[7].trim().to_string();
            if title.is_empty() {
                return None;
            }
            // Infer monitor from x position.
            if x < 0 || x >= primary_width {
                // Get app name from WM_CLASS via xprop.
                let app_name = std::process::Command::new("xprop")
                    .args(["-id", win_id, "WM_CLASS"])
                    .output()
                    .ok()
                    .filter(|o| o.status.success())
                    .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                    .and_then(|s| s.split('"').nth(3).map(|n| n.to_string()))
                    .unwrap_or_else(|| title.clone());
                Some(SecondaryWindowInfo {
                    app_name,
                    window_title: title,
                    monitor_id: if x < 0 { 2 } else { 1 },
                })
            } else {
                None
            }
        })
        .collect()
}

/// Poll visible windows on non-primary monitors (Windows).
/// Uses EnumWindows to list all visible windows and GetWindowRect for positions.
#[cfg(target_os = "windows")]
fn poll_secondary_windows() -> Vec<SecondaryWindowInfo> {
    type Hwnd = *mut core::ffi::c_void;
    type Bool = i32;
    type Lparam = isize;
    type Wchar = u16;

    #[repr(C)]
    #[derive(Default)]
    struct Rect {
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
    }

    #[link(name = "user32")]
    extern "system" {
        fn EnumWindows(callback: extern "system" fn(Hwnd, Lparam) -> Bool, lparam: Lparam) -> Bool;
        fn IsWindowVisible(hwnd: Hwnd) -> Bool;
        fn GetWindowTextW(hwnd: Hwnd, lp_string: *mut Wchar, n_max_count: i32) -> i32;
        fn GetWindowTextLengthW(hwnd: Hwnd) -> i32;
        fn GetWindowRect(hwnd: Hwnd, lp_rect: *mut Rect) -> Bool;
        fn GetForegroundWindow() -> Hwnd;
    }

    let mut results: Vec<SecondaryWindowInfo> = Vec::new();
    // Query primary monitor width via GetSystemMetrics(SM_CXSCREEN).
    #[link(name = "user32")]
    extern "system" {
        fn GetSystemMetrics(n_index: i32) -> i32;
    }
    const SM_CXSCREEN: i32 = 0;
    let primary_width: i32 = unsafe { GetSystemMetrics(SM_CXSCREEN) }.max(1920);

    unsafe {
        let fg = GetForegroundWindow();

        extern "system" fn enum_callback(hwnd: Hwnd, lparam: Lparam) -> Bool {
            unsafe {
                let data = &mut *(lparam as *mut (Vec<SecondaryWindowInfo>, Hwnd, i32));
                if hwnd == data.1 {
                    return 1; // skip foreground
                }
                if IsWindowVisible(hwnd) == 0 {
                    return 1;
                }
                let title_len = GetWindowTextLengthW(hwnd);
                if title_len <= 0 {
                    return 1;
                }
                let mut buf = vec![0u16; (title_len + 1) as usize];
                let len = GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32);
                if len <= 0 {
                    return 1;
                }
                let title = String::from_utf16_lossy(&buf[..len as usize]);

                let mut rect = Rect::default();
                if GetWindowRect(hwnd, &mut rect) != 0 && (rect.left < 0 || rect.left >= data.2) {
                    data.0.push(SecondaryWindowInfo {
                        app_name: title.clone(),
                        window_title: title,
                        monitor_id: if rect.left < 0 { 2 } else { 1 },
                    });
                }
            }
            1
        }

        let mut data = (results, fg, primary_width);
        EnumWindows(enum_callback, &mut data as *mut _ as Lparam);
        results = data.0;
    }

    results
}

/// Stub for unsupported platforms.
#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
fn poll_secondary_windows() -> Vec<SecondaryWindowInfo> {
    vec![]
}

#[cfg(target_os = "linux")]
fn poll_active_window() -> Option<ActiveWindowInfo> {
    let win_id_out = std::process::Command::new("xdotool")
        .arg("getactivewindow")
        .output()
        .ok()
        .filter(|o| o.status.success())?;
    let win_id = String::from_utf8_lossy(&win_id_out.stdout).trim().to_string();
    if win_id.is_empty() {
        return None;
    }

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

    // Try to find an open document file via /proc/PID/fd.
    let document_path = pid_prop
        .split('=')
        .nth(1)
        .and_then(|s| s.trim().parse::<u32>().ok())
        .and_then(|pid| linux_proc_document(pid));

    Some(ActiveWindowInfo {
        app_name,
        app_path,
        window_title,
        document_path,
        activated_at: unix_secs(),
        browser_title: None,
        monitor_id: None,
    })
}

/// On Linux, scan /proc/PID/fd for a recently-opened regular file in a
/// user directory.  Returns the first match (heuristic, not perfect).
#[cfg(target_os = "linux")]
fn linux_proc_document(pid: u32) -> Option<String> {
    let fd_dir = format!("/proc/{pid}/fd");
    let entries = std::fs::read_dir(&fd_dir).ok()?;
    for entry in entries.flatten() {
        let link = std::fs::read_link(entry.path()).ok()?;
        let path = link.to_string_lossy();
        // Skip non-regular or system paths.
        if path.starts_with("/dev")
            || path.starts_with("/proc")
            || path.starts_with("/sys")
            || path.starts_with("/tmp")
            || path.contains("/lib/")
            || path.contains("/lib64/")
            || path.contains(".so")
            || path.ends_with(".cache")
        {
            continue;
        }
        if link.is_file() && looks_like_file(&path) {
            return Some(path.to_string());
        }
    }
    None
}

#[cfg(target_os = "windows")]
fn poll_active_window() -> Option<ActiveWindowInfo> {
    type Hwnd = *mut core::ffi::c_void;
    type Handle = *mut core::ffi::c_void;
    type Dword = u32;
    type Bool = i32;
    type Wchar = u16;

    const PROCESS_QUERY_LIMITED_INFORMATION: Dword = 0x1000;

    #[link(name = "user32")]
    extern "system" {
        fn GetForegroundWindow() -> Hwnd;
        fn GetWindowTextW(hwnd: Hwnd, lp_string: *mut Wchar, n_max_count: i32) -> i32;
        fn GetWindowThreadProcessId(hwnd: Hwnd, lpdw_process_id: *mut Dword) -> Dword;
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn OpenProcess(dw_desired_access: Dword, b_inherit_handle: Bool, dw_process_id: Dword) -> Handle;
        fn QueryFullProcessImageNameW(
            h_process: Handle,
            dw_flags: Dword,
            lp_exe_name: *mut Wchar,
            lpdw_size: *mut Dword,
        ) -> Bool;
        fn CloseHandle(h_object: Handle) -> Bool;
    }

    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.is_null() {
            return None;
        }

        let mut title_buf = [0u16; 512];
        let title_len = GetWindowTextW(hwnd, title_buf.as_mut_ptr(), title_buf.len() as i32);
        let window_title = if title_len > 0 {
            String::from_utf16_lossy(&title_buf[..title_len as usize])
        } else {
            String::new()
        };

        let mut pid: Dword = 0;
        let _ = GetWindowThreadProcessId(hwnd, &mut pid);
        if pid == 0 {
            return None;
        }

        let h_process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if h_process.is_null() {
            return None;
        }

        let mut path_buf = [0u16; 1024];
        let mut size: Dword = path_buf.len() as Dword;
        let ok = QueryFullProcessImageNameW(h_process, 0, path_buf.as_mut_ptr(), &mut size) != 0;
        let _ = CloseHandle(h_process);
        if !ok || size == 0 {
            return None;
        }

        let app_path = String::from_utf16_lossy(&path_buf[..size as usize]);
        let app_name = std::path::Path::new(&app_path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        if app_name.is_empty() {
            return None;
        }

        Some(ActiveWindowInfo {
            app_name,
            app_path,
            window_title,
            document_path: None,
            activated_at: unix_secs(),
            browser_title: None,
        })
    }
}

#[cfg(target_os = "macos")]
fn poll_input_activity() -> (bool, bool) {
    type CfgEventType = u32;

    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        fn CGEventSourceSecondsSinceLastEventType(state_id: i32, event_type: CfgEventType) -> f64;
    }

    const KCG_EVENT_SOURCE_STATE_HID_SYSTEM_STATE: i32 = 1;
    const KCG_EVENT_KEY_DOWN: CfgEventType = 10;
    const KCG_EVENT_LEFT_MOUSE_DOWN: CfgEventType = 1;

    // SAFETY: CGEventSourceSecondsSinceLastEventType is a thread-safe CoreGraphics query.
    unsafe {
        let kbd_idle =
            CGEventSourceSecondsSinceLastEventType(KCG_EVENT_SOURCE_STATE_HID_SYSTEM_STATE, KCG_EVENT_KEY_DOWN);
        let mouse_idle =
            CGEventSourceSecondsSinceLastEventType(KCG_EVENT_SOURCE_STATE_HID_SYSTEM_STATE, KCG_EVENT_LEFT_MOUSE_DOWN);
        (kbd_idle < ACTIVE_THRESHOLD_SECS, mouse_idle < ACTIVE_THRESHOLD_SECS)
    }
}

#[cfg(target_os = "linux")]
fn poll_input_activity() -> (bool, bool) {
    let out = std::process::Command::new("xprintidle").output();
    let Ok(out) = out else {
        return (false, false);
    };
    if !out.status.success() {
        return (false, false);
    }

    let ms: f64 = String::from_utf8_lossy(&out.stdout).trim().parse().unwrap_or(f64::MAX);
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

    unsafe {
        let mut info = Lastinputinfo {
            cb_size: mem::size_of::<Lastinputinfo>() as u32,
            dw_time: 0,
        };
        if GetLastInputInfo(&mut info) == 0 {
            return (false, false);
        }
        let now_tick = GetTickCount();
        let idle_ms = now_tick.wrapping_sub(info.dw_time) as f64;
        let active = idle_ms < (ACTIVE_THRESHOLD_SECS * 1_000.0);
        (active, active)
    }
}

/// Tracks the live state of a focused file, producing 5-second edit chunks.
struct FileSnapshot {
    row_id: i64,
    path: String,
    category: String,
    start_ts: u64,
    initial_size: u64,
    initial_word_count: u64,
    /// Rolling state — updated after each chunk diff.
    prev_mtime: u64,
    prev_size: u64,
    prev_hashes: Vec<u64>,
    last_chunk_ts: u64,
    total_lines_added: u64,
    total_lines_removed: u64,
    total_undo_estimate: u64,
    was_modified: bool,
    /// Accumulated EEG samples for averaging over the interaction duration.
    eeg_focus_sum: f64,
    eeg_mood_sum: f64,
    eeg_focus_count: u32,
    eeg_mood_count: u32,
}

const CHUNK_INTERVAL_SECS: u64 = 5;

impl FileSnapshot {
    fn capture(path: &str, category: &str, row_id: i64, now: u64) -> Self {
        let (mtime, size) = file_mtime_and_size(path);
        let is_text = is_text_category(category);
        let hashes = if is_text { hash_lines(path) } else { vec![] };
        let word_count = if matches!(category, "document" | "spreadsheet") {
            count_words(path)
        } else {
            0
        };
        Self {
            row_id,
            path: path.to_string(),
            category: category.to_string(),
            start_ts: now,
            initial_size: size,
            initial_word_count: word_count,
            prev_mtime: mtime,
            prev_size: size,
            prev_hashes: hashes,
            last_chunk_ts: now,
            total_lines_added: 0,
            total_lines_removed: 0,
            total_undo_estimate: 0,
            was_modified: false,
            eeg_focus_sum: 0.0,
            eeg_mood_sum: 0.0,
            eeg_focus_count: 0,
            eeg_mood_count: 0,
        }
    }

    /// Accumulate an EEG sample into the running average.
    fn sample_eeg(&mut self, state: &AppState) {
        let (focus, mood) = read_eeg_snapshot(state);
        if let Some(f) = focus {
            self.eeg_focus_sum += f as f64;
            self.eeg_focus_count += 1;
        }
        if let Some(m) = mood {
            self.eeg_mood_sum += m as f64;
            self.eeg_mood_count += 1;
        }
    }

    fn maybe_emit_chunk(&mut self, store: &ActivityStore, state: &AppState) {
        let now = unix_secs();
        if now < self.last_chunk_ts + CHUNK_INTERVAL_SECS {
            return;
        }
        // Sample EEG every chunk tick (~5s) for duration-averaged focus/mood.
        self.sample_eeg(state);
        self.last_chunk_ts = now;

        let (cur_mtime, cur_size) = file_mtime_and_size(&self.path);
        if cur_mtime == self.prev_mtime {
            return;
        }
        self.was_modified = true;

        let is_text = is_text_category(&self.category);
        if is_text {
            let new_hashes = hash_lines(&self.path);
            let (added, removed) = diff_line_hashes(&self.prev_hashes, &new_hashes);
            let size_delta = cur_size as i64 - self.prev_size as i64;
            // Undo heuristic: when lines are both added and removed in the same
            // 5-second window and the file size barely changed, it suggests
            // undo/redo activity.  min(added, removed) = reversal count.
            let undo_est = if added > 0 && removed > 0 && size_delta.unsigned_abs() <= 10 {
                added.min(removed)
            } else {
                0
            };
            if added > 0 || removed > 0 {
                store.insert_edit_chunk(self.row_id, now, added, removed, size_delta, undo_est);
                self.total_lines_added += added;
                self.total_lines_removed += removed;
                self.total_undo_estimate += undo_est;
            }
            self.prev_hashes = new_hashes;
        } else {
            // Binary files — record size change only.
            let size_delta = cur_size as i64 - self.prev_size as i64;
            if size_delta != 0 {
                store.insert_edit_chunk(self.row_id, now, 0, 0, size_delta, 0);
            }
        }

        self.prev_mtime = cur_mtime;
        self.prev_size = cur_size;
    }

    fn finalize(mut self, store: &ActivityStore, state: &AppState) {
        // Take one final EEG sample before averaging.
        self.sample_eeg(state);
        let (cur_mtime, cur_size) = file_mtime_and_size(&self.path);
        if cur_mtime != self.prev_mtime {
            self.was_modified = true;
            let is_text = is_text_category(&self.category);
            if is_text {
                let new_hashes = hash_lines(&self.path);
                let (added, removed) = diff_line_hashes(&self.prev_hashes, &new_hashes);
                let size_delta = cur_size as i64 - self.prev_size as i64;
                let undo_est = if added > 0 && removed > 0 && size_delta.unsigned_abs() <= 10 {
                    added.min(removed)
                } else {
                    0
                };
                if added > 0 || removed > 0 {
                    store.insert_edit_chunk(self.row_id, unix_secs(), added, removed, size_delta, undo_est);
                    self.total_lines_added += added;
                    self.total_lines_removed += removed;
                    self.total_undo_estimate += undo_est;
                }
            }
        }

        let elapsed = unix_secs().saturating_sub(self.start_ts);
        let total_size_delta = cur_size as i64 - self.initial_size as i64;
        let words_delta = if self.was_modified && matches!(self.category.as_str(), "document" | "spreadsheet") {
            count_words(&self.path) as i64 - self.initial_word_count as i64
        } else {
            0
        };

        let avg_focus = if self.eeg_focus_count > 0 {
            Some((self.eeg_focus_sum / self.eeg_focus_count as f64) as f32)
        } else {
            None
        };
        let avg_mood = if self.eeg_mood_count > 0 {
            Some((self.eeg_mood_sum / self.eeg_mood_count as f64) as f32)
        } else {
            None
        };

        store.finalize_file_interaction(
            self.row_id,
            elapsed,
            self.was_modified,
            total_size_delta,
            self.total_lines_added,
            self.total_lines_removed,
            words_delta,
            self.total_undo_estimate,
            avg_focus,
            avg_mood,
        );
    }
}

/// Return (mtime_unix_secs, size_bytes) for a file, or (0, 0) on error.
fn file_mtime_and_size(path: &str) -> (u64, u64) {
    let Ok(meta) = std::fs::metadata(path) else {
        return (0, 0);
    };
    let mtime = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);
    (mtime, meta.len())
}

/// Whether this category contains text files suitable for line-level diffing.
fn is_text_category(category: &str) -> bool {
    matches!(category, "code" | "document" | "data" | "config" | "other" | "")
}

/// Count whitespace-separated words in a text file.
/// For .txt, .md, .rtf, etc.  Capped at 10 MB.  Returns 0 for binary files.
fn count_words(path: &str) -> u64 {
    use std::io::Read;
    let Ok(mut f) = std::fs::File::open(path) else { return 0 };
    let mut buf = String::new();
    if Read::take(&mut f, 10 * 1024 * 1024).read_to_string(&mut buf).is_err() {
        return 0;
    }
    buf.split_whitespace().count() as u64
}

/// Hash each line of a file into a `Vec<u64>`.  Returns an empty vec for
/// binary or unreadable files.  Reads at most 10 MB.
fn hash_lines(path: &str) -> Vec<u64> {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    use std::io::{BufRead, BufReader, Read};

    let Ok(f) = std::fs::File::open(path) else {
        return vec![];
    };
    let reader = BufReader::new(Read::take(f, 10 * 1024 * 1024));
    let mut hashes = Vec::new();
    for line in reader.lines() {
        let Ok(line) = line else {
            return vec![]; // binary file
        };
        let mut h = DefaultHasher::new();
        line.hash(&mut h);
        hashes.push(h.finish());
    }
    hashes
}

/// Compute (lines_added, lines_removed) by diffing two sequences of line
/// hashes.  Uses multiset comparison: counts how many lines (by content)
/// appeared or disappeared, regardless of position.  This means moving a
/// line registers as 0 changes, and replacing a line registers as 1 added +
/// 1 removed — which matches what the user typically cares about.
fn diff_line_hashes(old: &[u64], new: &[u64]) -> (u64, u64) {
    use std::collections::HashMap;

    // Build frequency map for old lines.
    let mut old_counts: HashMap<u64, i64> = HashMap::new();
    for &h in old {
        *old_counts.entry(h).or_default() += 1;
    }

    // Subtract new lines from the old counts.
    for &h in new {
        *old_counts.entry(h).or_default() -= 1;
    }

    // Positive remainder = lines that were removed (present in old, not in new).
    // Negative remainder = lines that were added (present in new, not in old).
    let mut added = 0u64;
    let mut removed = 0u64;
    for &diff in old_counts.values() {
        if diff > 0 {
            removed += diff as u64;
        } else if diff < 0 {
            added += (-diff) as u64;
        }
    }
    (added, removed)
}

fn run_poller(state: AppState, store: Arc<ActivityStore>) {
    let mut last: Option<ActiveWindowInfo> = None;
    let mut last_file: Option<String> = None;
    let mut snapshot: Option<FileSnapshot> = None;
    let mut last_prune: u64 = 0;
    // Meeting state tracking.
    let mut active_meeting: Option<(i64, &'static str)> = None; // (row_id, platform)

    // Load settings for file tracking.
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let settings = skill_settings::load_settings(&skill_dir);

    let rules = if settings.file_patterns.is_empty() {
        skill_settings::default_file_patterns()
    } else {
        settings.file_patterns
    };
    let mut engine = FilePatternEngine::compile(&rules);
    let mut excludes = compile_excludes(&settings.file_exclude_patterns);
    let mut retention_days = settings.file_retention_days;
    let mut settings_gen = state.settings_generation.load(Ordering::Relaxed);

    loop {
        std::thread::sleep(Duration::from_secs(1));

        if !state.track_active_window.load(Ordering::Relaxed) {
            if let Some(snap) = snapshot.take() {
                snap.finalize(&store, &state);
            }
            last = None;
            last_file = None;
            continue;
        }

        // ── Hot-reload patterns on settings change ───────────────────
        let gen = state.settings_generation.load(Ordering::Relaxed);
        if gen != settings_gen {
            settings_gen = gen;
            let s = skill_settings::load_settings(&skill_dir);
            let new_rules = if s.file_patterns.is_empty() {
                skill_settings::default_file_patterns()
            } else {
                s.file_patterns
            };
            engine = FilePatternEngine::compile(&new_rules);
            excludes = compile_excludes(&s.file_exclude_patterns);
            retention_days = s.file_retention_days;
        }

        let mut current = poll_active_window();
        let changed = match (&last, &current) {
            (None, None) => false,
            (None, Some(_)) => true,
            (Some(_), None) => true,
            (Some(prev), Some(cur)) => prev.app_name != cur.app_name || prev.window_title != cur.window_title,
        };

        if changed {
            if let Some(info) = &mut current {
                // Enrich with browser page title.
                info.browser_title = extract_browser_title(&info.app_name, &info.window_title);
                if let Some(primary_id) = store.insert_active_window(info) {
                    // Poll secondary monitor windows on window change.
                    let secondary = poll_secondary_windows();
                    if !secondary.is_empty() {
                        store.insert_secondary_windows(primary_id, &secondary);
                    }
                }
            }
            last.clone_from(&current);
        }

        // ── File interaction tracking ────────────────────────────────
        if state.track_file_activity.load(Ordering::Relaxed) {
            if let Some(info) = &current {
                // Prefer OS-reported document path; fall back to pattern engine.
                let file = info
                    .document_path
                    .clone()
                    .filter(|p| !p.is_empty() && file_exists(p))
                    .or_else(|| {
                        engine
                            .extract(&info.app_name, &info.window_title)
                            .filter(|p| file_exists(p))
                    })
                    .filter(|p| !is_excluded(p, &excludes));

                let file_changed = file != last_file;
                if file_changed {
                    let now = unix_secs();
                    // Finalize the previous file interaction.
                    if let Some(snap) = snapshot.take() {
                        snap.finalize(&store, &state);
                    }

                    if let Some(ref path) = file {
                        let project = detect_project(path);
                        let (language, category) = detect_language(path);
                        let git_branch = detect_git_branch(path);
                        let (eeg_focus, eeg_mood) = read_eeg_snapshot(&state);
                        if let Some(row_id) = store.insert_file_interaction(
                            path,
                            &info.app_name,
                            &project,
                            &language,
                            &category,
                            &git_branch,
                            now,
                            eeg_focus,
                            eeg_mood,
                        ) {
                            snapshot = Some(FileSnapshot::capture(path, &category, row_id, now));
                        }
                    }
                    last_file = file;
                } else if let Some(ref mut snap) = snapshot {
                    // Same file still focused — check for edits every 5s.
                    snap.maybe_emit_chunk(&store, &state);
                }
            } else {
                if let Some(snap) = snapshot.take() {
                    snap.finalize(&store, &state);
                }
                last_file = None;
            }
        }

        // ── Build/test detection from terminal titles ───────────────
        if let Some(info) = &current {
            if let Some((cmd, outcome)) = detect_build_event(&info.app_name, &info.window_title) {
                let project = last_file.as_deref().map(|p| detect_project(p)).unwrap_or_default();
                store.insert_build_event(&cmd, &outcome, &project, unix_secs());
            }
        }

        // ── Meeting / call detection ────────────────────────────────
        if let Some(info) = &current {
            let meeting = detect_meeting(&info.app_name, &info.window_title);
            match (meeting, &active_meeting) {
                (Some(platform), None) => {
                    // Meeting started.
                    if let Some(id) =
                        store.insert_meeting_start(platform, &info.window_title, &info.app_name, unix_secs())
                    {
                        active_meeting = Some((id, platform));
                        auto_label_eeg(&skill_dir, &format!("meeting start: {platform}"), &info.window_title);
                    }
                }
                (None, Some((id, _))) => {
                    // Meeting ended.
                    store.update_meeting_end(*id, unix_secs());
                    active_meeting = None;
                    auto_label_eeg(&skill_dir, "meeting end", "");
                }
                (Some(new_plat), Some((id, old_plat))) if new_plat != *old_plat => {
                    // Switched meeting platforms — end old, start new.
                    store.update_meeting_end(*id, unix_secs());
                    if let Some(new_id) =
                        store.insert_meeting_start(new_plat, &info.window_title, &info.app_name, unix_secs())
                    {
                        active_meeting = Some((new_id, new_plat));
                    }
                }
                _ => {} // Same meeting continues, or no meeting.
            }
        } else if let Some((id, _)) = active_meeting.take() {
            // Window went to None — end any active meeting.
            store.update_meeting_end(id, unix_secs());
        }

        // ── Periodic maintenance (once per hour) ──────────────────
        let now = unix_secs();
        if now >= last_prune + 3600 {
            last_prune = now;
            // Retention pruning.
            if retention_days > 0 {
                let cutoff = now.saturating_sub(retention_days as u64 * 86400);
                let deleted = store.prune_file_interactions(cutoff);
                store.prune_meetings(cutoff);
                store.prune_clipboard(cutoff);
                store.prune_secondary_windows(cutoff);
                store.prune_terminal_commands(cutoff);
                store.prune_ai_events(cutoff);
                store.prune_zone_switches(cutoff);
                store.prune_layout_snapshots(cutoff);
                if deleted > 0 {
                    tracing::info!("[activity] pruned {deleted} file_interactions older than {retention_days}d");
                }
            }
            // Build focus sessions from recent interactions.
            build_focus_sessions(&store, now.saturating_sub(7200));
            // Reclaim space from pruned rows (incremental auto-vacuum).
            store.optimize();
        }
    }
}

/// Extract the language name and broad category from the file extension.
fn detect_language(file_path: &str) -> (String, String) {
    let ext = std::path::Path::new(file_path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    let (lang, cat) = match ext.as_str() {
        // Code
        "rs" => ("rust", "code"),
        "py" => ("python", "code"),
        "js" => ("javascript", "code"),
        "ts" => ("typescript", "code"),
        "tsx" => ("typescript", "code"),
        "jsx" => ("javascript", "code"),
        "go" => ("go", "code"),
        "rb" => ("ruby", "code"),
        "java" => ("java", "code"),
        "kt" => ("kotlin", "code"),
        "swift" => ("swift", "code"),
        "c" => ("c", "code"),
        "cpp" | "cc" | "cxx" => ("cpp", "code"),
        "h" | "hpp" => ("cpp", "code"),
        "cs" => ("csharp", "code"),
        "php" => ("php", "code"),
        "r" => ("r", "code"),
        "scala" => ("scala", "code"),
        "ex" | "exs" => ("elixir", "code"),
        "erl" => ("erlang", "code"),
        "hs" => ("haskell", "code"),
        "ml" | "mli" => ("ocaml", "code"),
        "lua" => ("lua", "code"),
        "dart" => ("dart", "code"),
        "zig" => ("zig", "code"),
        "nim" => ("nim", "code"),
        "v" => ("v", "code"),
        "sh" | "bash" | "zsh" => ("shell", "code"),
        "ps1" => ("powershell", "code"),
        "sql" => ("sql", "code"),
        "proto" => ("protobuf", "code"),
        // Web
        "html" | "htm" => ("html", "code"),
        "css" | "scss" | "sass" | "less" => ("css", "code"),
        "svelte" => ("svelte", "code"),
        "vue" => ("vue", "code"),
        "astro" => ("astro", "code"),
        // Documents
        "docx" | "doc" => ("word", "document"),
        "rtf" => ("rtf", "document"),
        "odt" => ("odt", "document"),
        "pages" => ("pages", "document"),
        "txt" => ("text", "document"),
        "md" | "markdown" => ("markdown", "document"),
        "tex" | "latex" => ("latex", "document"),
        "pdf" => ("pdf", "document"),
        // Spreadsheets
        "xlsx" | "xls" => ("excel", "spreadsheet"),
        "csv" => ("csv", "spreadsheet"),
        "numbers" => ("numbers", "spreadsheet"),
        "ods" => ("ods", "spreadsheet"),
        "tsv" => ("tsv", "spreadsheet"),
        // Presentations
        "pptx" | "ppt" => ("powerpoint", "presentation"),
        "key" | "keynote" => ("keynote", "presentation"),
        "odp" => ("odp", "presentation"),
        // Images
        "png" | "jpg" | "jpeg" | "gif" | "svg" | "webp" | "bmp" | "tiff" | "ico" | "heic" => ("image", "image"),
        "raw" | "cr2" | "nef" | "dng" => ("raw", "image"),
        // Design
        "psd" => ("photoshop", "design"),
        "ai" => ("illustrator", "design"),
        "fig" => ("figma", "design"),
        "sketch" => ("sketch", "design"),
        "xd" => ("xd", "design"),
        "indd" => ("indesign", "design"),
        // Media
        "mp4" | "mov" | "avi" | "mkv" | "wmv" | "webm" => ("video", "media"),
        "mp3" | "wav" | "flac" | "aac" | "ogg" | "m4a" => ("audio", "media"),
        // Data
        "json" => ("json", "data"),
        "yaml" | "yml" => ("yaml", "data"),
        "toml" => ("toml", "data"),
        "xml" => ("xml", "data"),
        "sqlite" | "db" => ("database", "data"),
        "parquet" => ("parquet", "data"),
        // Config
        "dockerfile" => ("docker", "config"),
        "tf" | "hcl" => ("terraform", "config"),
        "ini" | "cfg" | "conf" => ("config", "config"),
        "env" => ("env", "config"),
        "lock" => ("lockfile", "config"),
        _ if ext.is_empty() => ("", ""),
        _ => (&ext as &str, "other"),
    };
    (lang.to_string(), cat.to_string())
}

/// Get the current git branch for a file's repository.
/// Only runs `git` if the file is inside a git repo (avoids spawning a
/// process for every document/image interaction).
fn detect_git_branch(file_path: &str) -> String {
    let path = std::path::Path::new(file_path);
    // Quick check: walk up looking for .git before shelling out.
    let mut dir = path.parent();
    let mut in_repo = false;
    while let Some(d) = dir {
        if d.join(".git").exists() {
            in_repo = true;
            break;
        }
        dir = d.parent();
    }
    if !in_repo {
        return String::new();
    }
    let dir = path.parent().unwrap_or(std::path::Path::new("."));
    std::process::Command::new("git")
        .args(["branch", "--show-current"])
        .current_dir(dir)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
}

/// Periodically write EEG snapshots to the timeseries table (every 5s when recording).
fn run_eeg_timeseries(state: AppState, store: Arc<ActivityStore>) {
    loop {
        std::thread::sleep(std::time::Duration::from_secs(5));
        let bands = state.latest_bands.lock().ok().and_then(|g| g.clone());
        if let Some(v) = bands {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            // Store the full band powers JSON as-is — extensible, any metric available
            let json = serde_json::to_string(&v).unwrap_or_default();
            if !json.is_empty() && json != "null" {
                store.insert_eeg_sample(now, &json);
            }
        }
    }
}

/// Read the current EEG focus (beta/alpha ratio) and mood from daemon state.
fn read_eeg_snapshot(state: &AppState) -> (Option<f32>, Option<f32>) {
    let bands = state.latest_bands.lock().ok().and_then(|g| g.clone());
    match bands {
        Some(v) => {
            let focus = v.get("bar").and_then(|v| v.as_f64()).map(|v| v as f32);
            let mood = v.get("mood").and_then(|v| v.as_f64()).map(|v| v as f32);
            (focus, mood)
        }
        None => (None, None),
    }
}

/// Detect the project or folder context for a file path.
/// For developer files: walks up looking for `.git`, `Cargo.toml`, etc.
/// For non-dev files: falls back to the immediate parent folder name
/// (e.g. "Invoices", "Photos", "School Work").
fn detect_project(file_path: &str) -> String {
    let markers = [
        ".git",
        "Cargo.toml",
        "package.json",
        "go.mod",
        "pyproject.toml",
        "setup.py",
        "pom.xml",
        "build.gradle",
        "*.xcodeproj",
        "*.sln",
        "Makefile",
        "CMakeLists.txt",
        "pubspec.yaml",
        "mix.exs",
    ];
    let path = std::path::Path::new(file_path);
    let mut dir = path.parent();
    while let Some(d) = dir {
        for marker in &markers {
            if let Some(suffix) = marker.strip_prefix('*') {
                if let Ok(entries) = std::fs::read_dir(d) {
                    for entry in entries.flatten() {
                        if entry.file_name().to_string_lossy().ends_with(suffix) {
                            return d.file_name().unwrap_or_default().to_string_lossy().to_string();
                        }
                    }
                }
            } else if d.join(marker).exists() {
                return d.file_name().unwrap_or_default().to_string_lossy().to_string();
            }
        }
        dir = d.parent();
    }
    // No dev markers found — use the immediate parent folder as context.
    path.parent()
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default()
}

/// Compile glob-style exclude patterns into regexes.
fn compile_excludes(patterns: &[String]) -> Vec<glob::Pattern> {
    patterns
        .iter()
        .filter_map(|p| {
            let expanded = if p.starts_with("~/") {
                normalise_tilde(p)
            } else {
                p.clone()
            };
            glob::Pattern::new(&expanded)
                .map_err(|e| tracing::warn!("bad exclude glob {p:?}: {e}"))
                .ok()
        })
        .collect()
}

/// Cluster recent file interactions into focus sessions.
/// A session boundary is a gap of >5 minutes between interactions.
fn build_focus_sessions(store: &ActivityStore, since: u64) {
    const GAP_THRESHOLD: u64 = 300; // 5 minutes

    let interactions = store.get_recent_files(1000, Some(since));
    if interactions.is_empty() {
        return;
    }

    // Interactions are newest-first; reverse for chronological order.
    let mut sorted: Vec<_> = interactions.into_iter().collect();
    sorted.sort_by_key(|r| r.seen_at);

    let mut session_start = sorted[0].seen_at;
    let mut session_end = sorted[0].seen_at + sorted[0].duration_secs.unwrap_or(0);
    let mut files: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut projects: std::collections::HashMap<String, u64> = std::collections::HashMap::new();
    let mut edit_count = 0u64;
    let mut lines_added = 0u64;
    let mut lines_removed = 0u64;
    let mut focus_sum = 0f64;
    let mut mood_sum = 0f64;
    let mut focus_count = 0u64;
    let mut mood_count = 0u64;

    let flush = |store: &ActivityStore,
                 start: u64,
                 end: u64,
                 files: &std::collections::HashSet<String>,
                 projects: &std::collections::HashMap<String, u64>,
                 edits: u64,
                 la: u64,
                 lr: u64,
                 fs: f64,
                 ms: f64,
                 fc: u64,
                 mc: u64| {
        if end <= start {
            return;
        }
        let main_project = projects
            .iter()
            .max_by_key(|(_, &v)| v)
            .map(|(k, _)| k.as_str())
            .unwrap_or("");
        let avg_focus = if fc > 0 { Some(fs as f32 / fc as f32) } else { None };
        let avg_mood = if mc > 0 { Some(ms as f32 / mc as f32) } else { None };
        store.insert_focus_session(
            start,
            end,
            main_project,
            files.len() as u64,
            edits,
            la,
            lr,
            avg_focus,
            avg_mood,
        );
    };

    for row in &sorted {
        let gap = row.seen_at.saturating_sub(session_end);
        if gap > GAP_THRESHOLD {
            // Flush previous session.
            flush(
                store,
                session_start,
                session_end,
                &files,
                &projects,
                edit_count,
                lines_added,
                lines_removed,
                focus_sum,
                mood_sum,
                focus_count,
                mood_count,
            );
            // Start new session.
            session_start = row.seen_at;
            files.clear();
            projects.clear();
            edit_count = 0;
            lines_added = 0;
            lines_removed = 0;
            focus_sum = 0.0;
            mood_sum = 0.0;
            focus_count = 0;
            mood_count = 0;
        }
        session_end = row.seen_at + row.duration_secs.unwrap_or(0);
        files.insert(row.file_path.clone());
        if !row.project.is_empty() {
            *projects.entry(row.project.clone()).or_default() += 1;
        }
        if row.was_modified {
            edit_count += 1;
        }
        lines_added += row.lines_added;
        lines_removed += row.lines_removed;
        if let Some(f) = row.eeg_focus {
            focus_sum += f as f64;
            focus_count += 1;
        }
        if let Some(m) = row.eeg_mood {
            mood_sum += m as f64;
            mood_count += 1;
        }
    }
    // Flush final session.
    flush(
        store,
        session_start,
        session_end,
        &files,
        &projects,
        edit_count,
        lines_added,
        lines_removed,
        focus_sum,
        mood_sum,
        focus_count,
        mood_count,
    );
}

/// Detect build/test commands and outcomes in terminal window titles.
/// Returns `Some((command, outcome))` if a build event is detected.
fn detect_build_event(app_name: &str, title: &str) -> Option<(String, String)> {
    let lower_app = app_name.to_lowercase();
    let is_terminal = lower_app.contains("terminal")
        || lower_app.contains("iterm")
        || lower_app.contains("alacritty")
        || lower_app.contains("kitty")
        || lower_app.contains("wezterm")
        || lower_app.contains("warp")
        || lower_app.contains("ghostty");
    if !is_terminal {
        return None;
    }
    let lower = title.to_lowercase();
    // Detect common build/test commands and their outcomes.
    let build_cmds = [
        "cargo build",
        "cargo test",
        "cargo check",
        "cargo clippy",
        "npm run",
        "npm test",
        "yarn build",
        "yarn test",
        "pnpm build",
        "make",
        "cmake",
        "go build",
        "go test",
        "pytest",
        "python -m pytest",
        "jest",
        "vitest",
        "gradle build",
        "mvn",
        "swift build",
        "xcodebuild",
    ];
    for cmd in &build_cmds {
        if lower.contains(cmd) {
            let outcome = if lower.contains("error") || lower.contains("failed") || lower.contains("fail") {
                "fail"
            } else if lower.contains("passed")
                || lower.contains("success")
                || lower.contains("ok")
                || lower.contains("finished")
            {
                "pass"
            } else {
                "running"
            };
            return Some((cmd.to_string(), outcome.to_string()));
        }
    }
    None
}

/// Detect meeting/call applications from window title.
/// Returns `Some(platform)` if a meeting is detected.
fn detect_meeting(app_name: &str, title: &str) -> Option<&'static str> {
    let lower_app = app_name.to_lowercase();
    let lower_title = title.to_lowercase();

    if lower_app.contains("zoom") && (lower_title.contains("meeting") || lower_title.contains("webinar")) {
        return Some("zoom");
    }
    if lower_app.contains("teams")
        && (lower_title.contains("meeting") || lower_title.contains("call") || lower_title.contains("| chat"))
    {
        return Some("teams");
    }
    if lower_app.contains("slack") && (lower_title.contains("huddle") || lower_title.contains("call")) {
        return Some("slack");
    }
    if lower_title.contains("meet.google.com") || lower_title.contains("google meet") {
        return Some("google_meet");
    }
    if lower_app.contains("facetime") {
        return Some("facetime");
    }
    if lower_app.contains("discord") && (lower_title.contains("voice") || lower_title.contains("stage")) {
        return Some("discord");
    }
    if lower_app.contains("webex") && (lower_title.contains("meeting") || lower_title.contains("call")) {
        return Some("webex");
    }
    None
}

/// Extract page title from browser window titles.
/// Most browsers use: "Page Title - Browser Name" or "Page Title — Browser Name".
/// Returns `Some(page_title)` for known browser apps.
fn extract_browser_title(app_name: &str, title: &str) -> Option<String> {
    let lower_app = app_name.to_lowercase();
    let is_browser = lower_app.contains("chrome")
        || lower_app.contains("safari")
        || lower_app.contains("firefox")
        || lower_app.contains("edge")
        || lower_app.contains("brave")
        || lower_app.contains("arc")
        || lower_app.contains("opera")
        || lower_app.contains("vivaldi")
        || lower_app.contains("chromium")
        || lower_app.contains("orion");
    if !is_browser {
        return None;
    }
    // Strip trailing " - BrowserName" or " — BrowserName" suffix.
    // Work backwards from the last separator.
    let separators = [" — ", " - ", " – "];
    for sep in &separators {
        if let Some(idx) = title.rfind(sep) {
            let page = title[..idx].trim();
            if !page.is_empty() {
                return Some(page.to_string());
            }
        }
    }
    // No separator found — use the full title.
    if !title.is_empty() {
        Some(title.to_string())
    } else {
        None
    }
}

/// Auto-label the EEG recording when significant activity events occur.
fn auto_label_eeg(skill_dir: &std::path::Path, text: &str, context: &str) {
    let db_path = skill_dir.join(skill_constants::LABELS_FILE);
    let conn = match rusqlite::Connection::open(&db_path) {
        Ok(c) => c,
        Err(_) => return,
    };
    let now = unix_secs() as i64;
    let _ = conn.execute(
        "INSERT INTO labels (text, context, eeg_start, eeg_end, wall_start, wall_end, created_at)
         VALUES (?1, ?2, ?3, ?3, ?3, ?3, ?3)",
        rusqlite::params![text, context, now],
    );
}

/// Check if a file path matches any exclude pattern.
fn is_excluded(path: &str, excludes: &[glob::Pattern]) -> bool {
    excludes.iter().any(|p| p.matches(path))
}

// ── Configurable file-pattern engine ─────────────────────────────────────────

/// A single compiled rule: pre-compiled regexes for app name and title.
struct CompiledRule {
    app_re: Regex,
    title_re: Regex,
}

/// Pre-compiled pattern engine built from [`FilePatternRule`] configs.
/// Evaluated in order; first match wins.
pub(crate) struct FilePatternEngine {
    rules: Vec<CompiledRule>,
}

impl FilePatternEngine {
    /// Compile a list of [`FilePatternRule`]s into a ready-to-use engine.
    /// Invalid regexes are logged and skipped.
    pub(crate) fn compile(rules: &[FilePatternRule]) -> Self {
        let compiled = rules
            .iter()
            .filter_map(|r| {
                let app_re = match Regex::new(&r.app) {
                    Ok(re) => re,
                    Err(e) => {
                        tracing::warn!("file_patterns: bad app regex {:?}: {e}", r.app);
                        return None;
                    }
                };
                let title_re = match Regex::new(&r.title) {
                    Ok(re) => re,
                    Err(e) => {
                        tracing::warn!("file_patterns: bad title regex {:?}: {e}", r.title);
                        return None;
                    }
                };
                Some(CompiledRule { app_re, title_re })
            })
            .collect();
        Self { rules: compiled }
    }

    /// Try to extract a file path from the given app name and window title.
    /// Returns `None` when no rule matches.
    pub(crate) fn extract(&self, app_name: &str, title: &str) -> Option<String> {
        if title.is_empty() {
            return None;
        }
        for rule in &self.rules {
            if !rule.app_re.is_match(app_name) {
                continue;
            }
            let Some(caps) = rule.title_re.captures(title) else {
                continue;
            };
            let file = match caps.name("file") {
                Some(m) => m.as_str().trim(),
                None => continue,
            };
            // Clean common decorations.
            let file = file
                .trim_start_matches('●')
                .trim_end_matches("[+]")
                .trim_end_matches("[modified]")
                .trim_end_matches("[Read-Only]")
                .trim_end_matches("[Compatibility Mode]")
                .trim_end_matches("[Protected View]")
                .trim();
            if file.is_empty() || file == "Welcome" || file == "Get Started" {
                continue;
            }
            // Check that it looks like a file (has an extension).
            if !looks_like_file(file) {
                continue;
            }
            let file = normalise_tilde(file);

            // If a `dir` capture group is present, join dir/file.
            if let Some(dir_m) = caps.name("dir") {
                let dir = dir_m.as_str().trim();
                if !dir.is_empty() {
                    let dir = normalise_tilde(dir);
                    // If the file is already absolute, skip dir.
                    if file.starts_with('/') {
                        return Some(file);
                    }
                    return Some(format!("{dir}/{file}"));
                }
            }
            return Some(file);
        }
        None
    }
}

/// Quick heuristic: does this string look like a filename (has a dot-extension)?
fn looks_like_file(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    // Take just the last path component.
    let name = s.rsplit('/').next().unwrap_or(s);
    // Must have a dot-extension with at least 1 char after the dot.
    if let Some(dot) = name.rfind('.') {
        let ext = &name[dot + 1..];
        !ext.is_empty() && ext.len() <= 12 && ext.chars().all(|c| c.is_ascii_alphanumeric())
    } else {
        false
    }
}

/// Check whether a path points to an existing regular file (not a directory).
/// Silently returns `false` for any I/O error.
fn file_exists(path: &str) -> bool {
    let p = std::path::Path::new(path);
    p.is_file()
}

/// Expand a leading `~` to the user's home directory.
fn normalise_tilde(s: &str) -> String {
    if let Some(rest) = s.strip_prefix("~/") {
        if let Some(home) = std::env::var("HOME").ok().or_else(|| dirs_next_home()) {
            return format!("{home}/{rest}");
        }
    }
    s.to_string()
}

#[cfg(not(target_os = "windows"))]
fn dirs_next_home() -> Option<String> {
    std::env::var("HOME").ok()
}

#[cfg(target_os = "windows")]
fn dirs_next_home() -> Option<String> {
    std::env::var("USERPROFILE").ok()
}

fn run_input_monitor(state: AppState, store: Arc<ActivityStore>) {
    let mut last_keyboard_ts: u64 = 0;
    let mut last_mouse_ts: u64 = 0;
    let mut kbd_count: u64 = 0;
    let mut mouse_count: u64 = 0;

    let mut prev_flush_kbd: u64 = 0;
    let mut prev_flush_mouse: u64 = 0;
    let mut last_flush_at: u64 = 0;

    loop {
        std::thread::sleep(Duration::from_secs(1));

        if !state.track_input_activity.load(Ordering::Relaxed) {
            continue;
        }

        let now = unix_secs();
        let (kbd_active, mouse_active) = poll_input_activity();

        if kbd_active {
            last_keyboard_ts = now;
            kbd_count = kbd_count.saturating_add(1);
        }
        if mouse_active {
            last_mouse_ts = now;
            mouse_count = mouse_count.saturating_add(1);
        }

        if now >= last_flush_at + 60 {
            last_flush_at = now;

            if last_keyboard_ts > 0 || last_mouse_ts > 0 {
                store.insert_input_activity(
                    if last_keyboard_ts > 0 {
                        Some(last_keyboard_ts)
                    } else {
                        None
                    },
                    if last_mouse_ts > 0 { Some(last_mouse_ts) } else { None },
                    now,
                );
            }

            let dk = kbd_count.saturating_sub(prev_flush_kbd);
            let dm = mouse_count.saturating_sub(prev_flush_mouse);
            prev_flush_kbd = kbd_count;
            prev_flush_mouse = mouse_count;

            if dk > 0 || dm > 0 {
                store.upsert_input_bucket(now / 60 * 60, dk, dm);
            }
        }
    }
}

/// Background filesystem watcher.  Watches common user directories for file
/// modifications and records them as file interactions.  Complements the
/// window-title-based approach by catching saves that happen outside the
/// focused window (e.g. auto-save, `git checkout`, build output).
fn run_file_watcher(state: AppState, store: Arc<ActivityStore>) {
    use notify::{Config, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
    use std::sync::mpsc;

    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();

    // Directories to watch — common user content locations.
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_default();
    if home.is_empty() {
        tracing::warn!("[file-watcher] HOME not set; file watcher disabled");
        return;
    }
    let watch_dirs: Vec<std::path::PathBuf> = [
        "Documents",
        "Desktop",
        "Downloads",
        "Projects",
        "Developer",
        "code",
        "src",
    ]
    .iter()
    .map(|d| std::path::PathBuf::from(&home).join(d))
    .filter(|p| p.is_dir())
    .collect();

    if watch_dirs.is_empty() {
        tracing::info!("[file-watcher] no user directories found to watch");
        return;
    }

    let (tx, rx) = mpsc::channel();

    let mut watcher = match RecommendedWatcher::new(tx, Config::default()) {
        Ok(w) => w,
        Err(e) => {
            tracing::warn!("[file-watcher] failed to create watcher: {e}");
            return;
        }
    };

    for dir in &watch_dirs {
        if let Err(e) = watcher.watch(dir, RecursiveMode::Recursive) {
            tracing::warn!("[file-watcher] failed to watch {}: {e}", dir.display());
        }
    }

    tracing::info!("[file-watcher] watching {} directories", watch_dirs.len());

    let settings = skill_settings::load_settings(&skill_dir);
    let excludes = compile_excludes(&settings.file_exclude_patterns);

    for event in rx {
        if !state.track_file_activity.load(Ordering::Relaxed) {
            continue;
        }
        let Ok(event) = event else { continue };
        // Only track content modifications, not metadata changes.
        let is_modify = matches!(
            event.kind,
            EventKind::Modify(notify::event::ModifyKind::Data(_)) | EventKind::Create(notify::event::CreateKind::File)
        );
        if !is_modify {
            continue;
        }
        for path in &event.paths {
            let path_str = path.to_string_lossy();
            if !looks_like_file(&path_str) {
                continue;
            }
            if is_excluded(&path_str, &excludes) {
                continue;
            }
            let project = detect_project(&path_str);
            let (language, category) = detect_language(&path_str);
            store.insert_file_interaction(
                &path_str,
                "filesystem",
                &project,
                &language,
                &category,
                "",
                unix_secs(),
                None,
                None,
            );
        }
    }
}

fn unix_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build an engine from the built-in default patterns.
    fn default_engine() -> FilePatternEngine {
        FilePatternEngine::compile(&skill_settings::default_file_patterns())
    }

    /// Shorthand: extract a file path using the default rule set.
    fn extract(app: &str, title: &str) -> Option<String> {
        default_engine().extract(app, title)
    }

    #[test]
    fn unix_secs_is_nonzero_and_monotonic() {
        let a = unix_secs();
        std::thread::sleep(Duration::from_millis(5));
        let b = unix_secs();
        assert!(a > 0);
        assert!(b >= a);
    }

    // ── Pattern engine tests ─────────────────────────────────────────────

    #[test]
    fn vscode_title_with_project() {
        let f = extract("Code", "main.rs — skill — Visual Studio Code");
        assert_eq!(f.as_deref(), Some("skill/main.rs"));
    }

    #[test]
    fn vscode_modified_indicator() {
        let f = extract("Code", "● lib.rs — myproj — Visual Studio Code");
        assert_eq!(f.as_deref(), Some("myproj/lib.rs"));
    }

    #[test]
    fn vscode_welcome_ignored() {
        assert!(extract("Code", "Welcome — Visual Studio Code").is_none());
    }

    #[test]
    fn cursor_title() {
        let f = extract("Cursor", "app.tsx — frontend — Cursor");
        assert_eq!(f.as_deref(), Some("frontend/app.tsx"));
    }

    #[test]
    fn jetbrains_with_bracket_path() {
        let f = extract("PyCharm", "main.py – myproject [~/Projects/myproject]");
        assert!(f.is_some());
        let p = f.unwrap();
        assert!(p.ends_with("/Projects/myproject/main.py"), "got: {p}");
    }

    #[test]
    fn xcode_title() {
        let f = extract("Xcode", "ViewController.swift — MyApp — Xcode");
        assert_eq!(f.as_deref(), Some("ViewController.swift"));
    }

    #[test]
    fn sublime_with_dir() {
        let f = extract("Sublime Text", "main.rs (~/code/proj) - Sublime Text");
        assert!(f.is_some());
        let p = f.unwrap();
        assert!(p.ends_with("/code/proj/main.rs"), "got: {p}");
    }

    #[test]
    fn terminal_vim_command() {
        let f = extract("iTerm2", "vim src/main.rs");
        assert_eq!(f.as_deref(), Some("src/main.rs"));
    }

    #[test]
    fn generic_absolute_path() {
        let f = extract("SomeApp", "Editing /Users/me/doc.txt");
        assert_eq!(f.as_deref(), Some("/Users/me/doc.txt"));
    }

    #[test]
    fn no_path_in_title() {
        assert!(extract("Safari", "Google - Safari").is_none());
    }

    #[test]
    fn empty_title() {
        assert!(extract("Finder", "").is_none());
    }

    #[test]
    fn looks_like_file_basic() {
        assert!(looks_like_file("main.rs"));
        assert!(looks_like_file("path/to/file.txt"));
        assert!(!looks_like_file("NoExtension"));
        assert!(!looks_like_file(""));
    }

    // ── Editor / app tests ───────────────────────────────────────────────

    #[test]
    fn zed_title() {
        let f = extract("Zed", "lib.rs — myproject — Zed");
        assert_eq!(f.as_deref(), Some("myproject/lib.rs"));
    }

    #[test]
    fn android_studio_title() {
        let f = extract(
            "Android Studio",
            "MainActivity.kt – myapp [~/AndroidStudioProjects/myapp]",
        );
        assert!(f.is_some());
        let p = f.unwrap();
        assert!(p.ends_with("/AndroidStudioProjects/myapp/MainActivity.kt"), "got: {p}");
    }

    #[test]
    fn emacs_title() {
        let f = extract("Emacs", "init.el  (Emacs@localhost)");
        assert_eq!(f.as_deref(), Some("init.el"));
    }

    #[test]
    fn helix_title() {
        let f = extract("Helix", "main.rs [+] — hx");
        assert_eq!(f.as_deref(), Some("main.rs"));
    }

    #[test]
    fn helix_title_reversed() {
        let f = extract("hx", "hx — src/lib.rs");
        assert_eq!(f.as_deref(), Some("src/lib.rs"));
    }

    #[test]
    fn office_word_title() {
        let f = extract("Microsoft Word", "Report.docx - Word");
        assert_eq!(f.as_deref(), Some("Report.docx"));
    }

    #[test]
    fn office_excel_readonly() {
        let f = extract("Microsoft Excel", "Budget.xlsx [Read-Only] - Excel");
        assert_eq!(f.as_deref(), Some("Budget.xlsx"));
    }

    #[test]
    fn adobe_photoshop() {
        let f = extract("Adobe Photoshop", "banner.psd @ 100% (RGB/8)");
        assert_eq!(f.as_deref(), Some("banner.psd"));
    }

    #[test]
    fn figma_title() {
        let f = extract("Figma", "Design System.fig — Figma");
        assert_eq!(f.as_deref(), Some("Design System.fig"));
    }

    #[test]
    fn obsidian_note_with_extension() {
        let f = extract("Obsidian", "todo.md - vault - Obsidian");
        assert_eq!(f.as_deref(), Some("todo.md"));
    }

    #[test]
    fn preview_title() {
        let f = extract("Preview", "screenshot.png");
        assert_eq!(f.as_deref(), Some("screenshot.png"));
    }

    #[test]
    fn textedit_title() {
        let f = extract("TextEdit", "notes.txt");
        assert_eq!(f.as_deref(), Some("notes.txt"));
    }

    #[test]
    fn pdf_reader_title() {
        let f = extract("Skim", "paper.pdf - Skim");
        assert_eq!(f.as_deref(), Some("paper.pdf"));
    }

    #[test]
    fn nova_title() {
        let f = extract("Nova", "index.html — mysite — Nova");
        assert_eq!(f.as_deref(), Some("mysite/index.html"));
    }

    #[test]
    fn terminal_nano_command() {
        let f = extract("Terminal", "nano config.yaml");
        assert_eq!(f.as_deref(), Some("config.yaml"));
    }

    #[test]
    fn terminal_hx_command() {
        let f = extract("Alacritty", "hx src/main.rs");
        assert_eq!(f.as_deref(), Some("src/main.rs"));
    }

    #[test]
    fn ghostty_terminal() {
        let f = extract("Ghostty", "nvim app.tsx");
        assert_eq!(f.as_deref(), Some("app.tsx"));
    }

    #[test]
    fn windsurf_title() {
        let f = extract("Windsurf", "server.ts — backend — Windsurf");
        assert_eq!(f.as_deref(), Some("backend/server.ts"));
    }

    #[test]
    fn sketch_title() {
        let f = extract("Sketch", "mockup.sketch — Page 1");
        assert_eq!(f.as_deref(), Some("mockup.sketch"));
    }

    #[test]
    fn iwork_pages() {
        let f = extract("Pages", "letter.pages — Pages");
        assert_eq!(f.as_deref(), Some("letter.pages"));
    }

    // ── Custom pattern test ──────────────────────────────────────────────

    #[test]
    fn custom_user_pattern() {
        let rules = vec![FilePatternRule {
            app: r"(?i)myeditor".into(),
            title: r"FILE=(?P<file>[^\s]+)".into(),
            comment: "Custom editor with FILE= prefix".into(),
        }];
        let engine = FilePatternEngine::compile(&rules);
        let f = engine.extract("MyEditor", "FILE=/home/user/notes.txt something");
        assert_eq!(f.as_deref(), Some("/home/user/notes.txt"));
    }

    #[test]
    fn custom_pattern_with_dir() {
        let rules = vec![FilePatternRule {
            app: r".*".into(),
            title: r"\[(?P<dir>[^\]]+)\]\s*(?P<file>\S+\.\w+)".into(),
            comment: "[dir] file pattern".into(),
        }];
        let engine = FilePatternEngine::compile(&rules);
        let f = engine.extract("AnyApp", "[/home/user] notes.txt");
        assert_eq!(f.as_deref(), Some("/home/user/notes.txt"));
    }

    #[test]
    fn bad_regex_skipped() {
        let rules = vec![
            FilePatternRule {
                app: r"[invalid".into(),
                title: r"(?P<file>.+)".into(),
                comment: "bad app regex".into(),
            },
            FilePatternRule {
                app: r".*".into(),
                title: r"^(?P<file>.+\.\w+)$".into(),
                comment: "fallback".into(),
            },
        ];
        let engine = FilePatternEngine::compile(&rules);
        // Bad regex is skipped, fallback works.
        let f = engine.extract("Any", "notes.txt");
        assert_eq!(f.as_deref(), Some("notes.txt"));
    }

    // ── Diff tests ───────────────────────────────────────────────────────

    #[test]
    fn diff_no_changes() {
        let old = vec![1, 2, 3];
        let (a, r) = diff_line_hashes(&old, &old);
        assert_eq!(a, 0);
        assert_eq!(r, 0);
    }

    #[test]
    fn diff_pure_additions() {
        let old = vec![1, 2];
        let new = vec![1, 2, 3, 4];
        let (a, r) = diff_line_hashes(&old, &new);
        assert_eq!(a, 2);
        assert_eq!(r, 0);
    }

    #[test]
    fn diff_pure_deletions() {
        let old = vec![1, 2, 3, 4];
        let new = vec![1, 2];
        let (a, r) = diff_line_hashes(&old, &new);
        assert_eq!(a, 0);
        assert_eq!(r, 2);
    }

    #[test]
    fn diff_replacements() {
        // Replace lines 2,3 with 5,6 — should show 2 added, 2 removed
        let old = vec![1, 2, 3, 4];
        let new = vec![1, 5, 6, 4];
        let (a, r) = diff_line_hashes(&old, &new);
        assert_eq!(a, 2);
        assert_eq!(r, 2);
    }

    #[test]
    fn diff_reorder_no_churn() {
        // Moving lines around doesn't count as changes
        let old = vec![1, 2, 3];
        let new = vec![3, 1, 2];
        let (a, r) = diff_line_hashes(&old, &new);
        assert_eq!(a, 0);
        assert_eq!(r, 0);
    }

    #[test]
    fn diff_empty_to_content() {
        let (a, r) = diff_line_hashes(&[], &[1, 2, 3]);
        assert_eq!(a, 3);
        assert_eq!(r, 0);
    }

    #[test]
    fn diff_content_to_empty() {
        let (a, r) = diff_line_hashes(&[1, 2, 3], &[]);
        assert_eq!(a, 0);
        assert_eq!(r, 3);
    }

    // ── Meeting detection tests ──────────────────────────────────────────────

    #[test]
    fn detect_zoom_meeting() {
        assert_eq!(detect_meeting("zoom.us", "Zoom Meeting"), Some("zoom"));
        assert_eq!(detect_meeting("zoom.us", "Zoom"), None);
    }

    #[test]
    fn detect_teams_call() {
        assert_eq!(detect_meeting("Microsoft Teams", "John Doe | Meeting"), Some("teams"));
    }

    #[test]
    fn detect_slack_huddle() {
        assert_eq!(detect_meeting("Slack", "Huddle in #general"), Some("slack"));
    }

    #[test]
    fn detect_google_meet_browser() {
        assert_eq!(
            detect_meeting("Google Chrome", "Meeting - meet.google.com"),
            Some("google_meet")
        );
    }

    #[test]
    fn no_meeting_in_editor() {
        assert_eq!(detect_meeting("Code", "main.rs — project — Visual Studio Code"), None);
    }

    // ── Browser title extraction tests ───────────────────────────────────────

    #[test]
    fn browser_title_chrome() {
        let t = extract_browser_title("Google Chrome", "GitHub - Pull Request #123 - Google Chrome");
        assert_eq!(t.as_deref(), Some("GitHub - Pull Request #123"));
    }

    #[test]
    fn browser_title_safari() {
        let t = extract_browser_title("Safari", "Apple Developer Documentation — Safari");
        assert_eq!(t.as_deref(), Some("Apple Developer Documentation"));
    }

    #[test]
    fn browser_title_not_browser() {
        assert_eq!(
            extract_browser_title("Code", "main.rs — project — Visual Studio Code"),
            None
        );
    }
}

// ── Clipboard monitor (macOS only) ───────────────────────────────────────────

#[cfg(target_os = "macos")]
fn run_clipboard_monitor(state: AppState, store: Arc<ActivityStore>) {
    let mut last_change_count: i64 = -1;
    let mut permission_denied_until: u64 = 0; // backoff when permission denied

    loop {
        std::thread::sleep(Duration::from_secs(2));

        // Check if clipboard tracking is enabled.
        let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
        let settings = skill_settings::load_settings(&skill_dir);
        if !settings.track_clipboard {
            continue;
        }

        // Backoff when permission was recently denied (re-check every 60s).
        let now = unix_secs();
        if now < permission_denied_until {
            continue;
        }

        // Query macOS pasteboard change count via osascript.
        // If Automation permission is not granted, this will fail.
        let out = match run_osascript("the clipboard info") {
            Some(s) => s,
            None => {
                // Permission denied or osascript failed/timed out — back off for 60s.
                permission_denied_until = now + 60;
                continue;
            }
        };

        // The output looks like: {{«class utf8», 42}, {string, 42}}
        // We hash the output to detect changes.
        let hash = {
            use std::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            out.hash(&mut h);
            h.finish() as i64
        };

        if hash == last_change_count {
            continue;
        }
        last_change_count = hash;

        // Determine content size from the info string.
        let content_size: u64 = out
            .split(',')
            .filter_map(|s| s.trim().trim_end_matches('}').trim().parse::<u64>().ok())
            .next()
            .unwrap_or(0);

        // Detect content type from clipboard info.
        let content_type = if out.contains("«class PNGf»") || out.contains("TIFF") {
            "image"
        } else if out.contains("«class furl»") {
            "file"
        } else {
            "text"
        };

        // Get current active window as the "source app".
        let source_app = poll_active_window().map(|w| w.app_name).unwrap_or_default();

        store.insert_clipboard_event(&source_app, content_type, content_size, unix_secs());

        // ── Clipboard image capture ──
        // When the clipboard contains an image and the feature is enabled,
        // extract the image data and import it into the screenshot store.
        if content_type == "image" {
            let settings = skill_settings::load_settings(&skill_dir);
            if settings.screenshot.clipboard_image_enabled {
                if let Some(png_path) = extract_clipboard_image_to_temp() {
                    let (eeg_focus, eeg_mood) = read_eeg_snapshot(&state);
                    if let Some(saved) =
                        skill_screenshots::user_screenshot::import_user_screenshot(&skill_dir, &png_path)
                    {
                        store.insert_user_screenshot_event(
                            saved.row_id,
                            unix_secs(),
                            &source_app,
                            "clipboard",
                            &png_path.to_string_lossy(),
                            "",
                            eeg_focus,
                            eeg_mood,
                        );
                        tracing::info!(
                            "[clipboard-image] captured clipboard image -> row_id={} (focus={:?})",
                            saved.row_id,
                            eeg_focus,
                        );
                    }
                    // Clean up temp file.
                    let _ = std::fs::remove_file(&png_path);
                }
            }
        }
    }
}

/// Extract clipboard image data to a temporary PNG file (macOS).
/// Returns the path to the temp file, or None if extraction fails.
#[cfg(target_os = "macos")]
fn extract_clipboard_image_to_temp() -> Option<std::path::PathBuf> {
    let tmp_dir = std::env::temp_dir();
    let tmp_path = tmp_dir.join(format!("skill_clipboard_{}.png", unix_secs()));
    // Use osascript to write clipboard PNG data to a file.
    let script = format!(
        r#"
        set pngData to the clipboard as «class PNGf»
        set filePath to POSIX file "{}"
        set fileRef to open for access filePath with write permission
        write pngData to fileRef
        close access fileRef
        "#,
        tmp_path.display()
    );
    match run_osascript(&script) {
        Some(_) if tmp_path.exists() => Some(tmp_path),
        _ => {
            let _ = std::fs::remove_file(&tmp_path);
            None
        }
    }
}

#[cfg(not(target_os = "macos"))]
fn extract_clipboard_image_to_temp() -> Option<std::path::PathBuf> {
    // TODO: Windows/Linux clipboard image extraction.
    None
}

// ── User screenshot watcher (cross-platform) ────────────────────────────────

fn run_user_screenshot_watcher(state: AppState, store: Arc<ActivityStore>) {
    use notify::{Config, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
    use std::collections::{HashMap, HashSet};
    use std::sync::mpsc;
    use std::time::Instant;

    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();

    // Wait for the feature to be enabled (poll every 5s).
    loop {
        let settings = skill_settings::load_settings(&skill_dir);
        if settings.screenshot.user_screenshot_enabled {
            break;
        }
        std::thread::sleep(Duration::from_secs(5));
    }

    // Detect screenshot directories.
    let watch_dirs = skill_screenshots::user_screenshot::detect_screenshot_dirs();
    if watch_dirs.is_empty() {
        tracing::info!("[user-screenshot] no screenshot directories found to watch");
        return;
    }

    let (tx, rx) = mpsc::channel();
    let mut watcher = match RecommendedWatcher::new(tx, Config::default()) {
        Ok(w) => w,
        Err(e) => {
            tracing::warn!("[user-screenshot] failed to create watcher: {e}");
            return;
        }
    };

    for dir in &watch_dirs {
        // Non-recursive — screenshot directories are flat.
        if let Err(e) = watcher.watch(dir, RecursiveMode::NonRecursive) {
            tracing::warn!("[user-screenshot] failed to watch {}: {e}", dir.display());
        } else {
            tracing::info!("[user-screenshot] watching {}", dir.display());
        }
    }

    let mut seen: HashSet<std::path::PathBuf> = HashSet::new();
    let mut debounce: HashMap<std::path::PathBuf, Instant> = HashMap::new();

    // Open a persistent ScreenshotStore handle for dedup checks.
    let ss_store = skill_data::screenshot_store::ScreenshotStore::open(&skill_dir);

    loop {
        // Use recv_timeout to periodically drain the debounce map.
        match rx.recv_timeout(Duration::from_millis(500)) {
            Ok(Ok(event)) => {
                let is_relevant = matches!(
                    event.kind,
                    EventKind::Create(_) | EventKind::Modify(notify::event::ModifyKind::Data(_))
                );
                if is_relevant {
                    for path in event.paths {
                        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                        if skill_screenshots::user_screenshot::is_user_screenshot(name) {
                            debounce.insert(path, Instant::now());
                        }
                    }
                }
            }
            Ok(Err(_)) | Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }

        // Process debounced files (1.5s since last event for that path).
        let now = Instant::now();
        let ready: Vec<std::path::PathBuf> = debounce
            .iter()
            .filter(|(_, ts)| now.duration_since(**ts) >= Duration::from_millis(1500))
            .map(|(p, _)| p.clone())
            .collect();

        for path in ready {
            debounce.remove(&path);

            if seen.contains(&path) {
                continue;
            }
            if !path.exists() || !path.is_file() {
                continue;
            }

            // Re-check config is still enabled.
            let settings = skill_settings::load_settings(&skill_dir);
            if !settings.screenshot.user_screenshot_enabled {
                continue;
            }

            // Dedup against SQLite (survives daemon restarts).
            let path_str = path.to_string_lossy().to_string();
            if let Some(ref store) = ss_store {
                if store.has_user_screenshot_from_path(&path_str) {
                    seen.insert(path.clone());
                    continue;
                }
            }

            // Capture context at the moment the user took the screenshot.
            let (eeg_focus, eeg_mood) = read_eeg_snapshot(&state);
            let aw = poll_active_window();
            let (aw_app, aw_title) = aw.map(|w| (w.app_name, w.window_title)).unwrap_or_default();

            // Import the screenshot into the screenshot store.
            match skill_screenshots::user_screenshot::import_user_screenshot(&skill_dir, &path) {
                Some(saved) => {
                    seen.insert(path.clone());

                    // Record as an activity event with EEG + window context.
                    let now_ts = unix_secs();
                    store.insert_user_screenshot_event(
                        saved.row_id,
                        now_ts,
                        &aw_app,
                        &aw_title,
                        &path_str,
                        "", // OCR preview filled later by embed backfill
                        eeg_focus,
                        eeg_mood,
                    );

                    tracing::info!(
                        "[user-screenshot] imported {} -> row_id={} (focus={:?})",
                        path.display(),
                        saved.row_id,
                        eeg_focus,
                    );
                }
                None => {
                    tracing::warn!("[user-screenshot] failed to import {}", path.display());
                }
            }
        }

        // Prevent unbounded growth of the seen set.
        if seen.len() > 10_000 {
            seen.clear();
        }
    }
}
