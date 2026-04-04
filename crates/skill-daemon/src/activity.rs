// SPDX-License-Identifier: GPL-3.0-only
//! Daemon-owned active-window and input-activity workers.

use std::{
    sync::{atomic::Ordering, Arc},
    time::Duration,
};

use skill_data::{active_window::ActiveWindowInfo, activity_store::ActivityStore};

use crate::state::AppState;

const ACTIVE_THRESHOLD_SECS: f64 = 2.0;

pub fn start_workers(state: AppState) {
    let skill_dir = state.skill_dir.lock().map(|g| g.clone()).unwrap_or_default();
    let Some(store) = ActivityStore::open(&skill_dir).map(Arc::new) else {
        tracing::warn!("activity store unavailable; activity workers disabled");
        return;
    };

    {
        let s = state.clone();
        let st = store.clone();
        std::thread::Builder::new()
            .name("daemon-active-window-poll".into())
            .spawn(move || run_poller(s, st))
            .expect("failed to spawn daemon active-window poller");
    }

    {
        let s = state;
        std::thread::Builder::new()
            .name("daemon-input-monitor".into())
            .spawn(move || run_input_monitor(s, store))
            .expect("failed to spawn daemon input monitor");
    }
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
    return appName & "|||" & appPath & "|||" & winTitle
end tell"#;

    let out = std::process::Command::new("osascript")
        .args(["-e", script])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }

    let raw = String::from_utf8_lossy(&out.stdout);
    let raw = raw.trim();
    let mut parts = raw.splitn(3, "|||");
    let app_name = parts.next().unwrap_or("").trim().to_string();
    let app_path = parts.next().unwrap_or("").trim().to_string();
    let window_title = parts.next().unwrap_or("").trim().to_string();
    if app_name.is_empty() {
        return None;
    }

    Some(ActiveWindowInfo {
        app_name,
        app_path,
        window_title,
        activated_at: unix_secs(),
    })
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

    Some(ActiveWindowInfo {
        app_name,
        app_path,
        window_title,
        activated_at: unix_secs(),
    })
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
            activated_at: unix_secs(),
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

fn run_poller(state: AppState, store: Arc<ActivityStore>) {
    let mut last: Option<ActiveWindowInfo> = None;

    loop {
        std::thread::sleep(Duration::from_secs(1));

        if !state.track_active_window.load(Ordering::Relaxed) {
            last = None;
            continue;
        }

        let current = poll_active_window();
        let changed = match (&last, &current) {
            (None, None) => false,
            (None, Some(_)) => true,
            (Some(_), None) => true,
            (Some(prev), Some(cur)) => prev.app_name != cur.app_name || prev.window_title != cur.window_title,
        };

        if changed {
            if let Some(info) = &current {
                store.insert_active_window(info);
            }
            last = current;
        }
    }
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

fn unix_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
