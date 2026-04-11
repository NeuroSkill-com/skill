// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
//! Platform-specific helpers (Linux decoration fix, appindicator probe).

// ── Linux decoration workaround (tauri-apps/tauri#11856) ─────────────────────
// On Linux (Wayland + GNOME/Mutter/KWin), window decorations (close /
// minimize / maximize buttons) become completely unresponsive when a window
// is created with `visible(false)` and later shown, or after any hide→show
// cycle.  Briefly toggling fullscreen after `show()` forces the compositor
// to re-evaluate the decoration state.  The toggle is near-instantaneous
// and visually imperceptible.  Must be called *after* `win.show()`.
#[cfg(target_os = "linux")]
pub(crate) fn linux_fix_decorations(win: &tauri::WebviewWindow) {
    eprintln!(
        "[linux] applying decoration fix (fullscreen toggle) for {}",
        win.label()
    );
    let _ = win.set_fullscreen(true);
    let _ = win.set_fullscreen(false);
}
#[cfg(not(target_os = "linux"))]
pub(crate) fn linux_fix_decorations(_win: &tauri::WebviewWindow) {}

#[cfg(target_os = "linux")]
pub(crate) fn linux_has_appindicator_runtime() -> bool {
    let candidates = [
        "libayatana-appindicator3.so.1",
        "libappindicator3.so.1",
        "libayatana-appindicator3.so",
        "libappindicator3.so",
    ];

    for name in candidates {
        let Ok(c_name) = std::ffi::CString::new(name) else {
            continue;
        };
        // SAFETY: `c_name` is a valid NUL-terminated C string that outlives the call.
        // `dlopen` with RTLD_LAZY|RTLD_LOCAL is safe for probing library availability.
        let handle = unsafe { libc::dlopen(c_name.as_ptr(), libc::RTLD_LAZY | libc::RTLD_LOCAL) };
        if !handle.is_null() {
            // SAFETY: `handle` is a non-null pointer returned by `dlopen` above.
            let _ = unsafe { libc::dlclose(handle) };
            return true;
        }
    }

    false
}

// ── Windows crash handler ────────────────────────────────────────────────────

/// Install a vectored exception handler for crash diagnostics (Windows only).
#[cfg(target_os = "windows")]
pub(crate) fn install_windows_crash_handler() {
    // SAFETY: Raw Win32 FFI for crash diagnostics. The handler only reads
    // exception pointers provided by the OS after a null check, and the
    // `AddVectoredExceptionHandler` call is safe with a valid function pointer.
    unsafe {
        #[repr(C)]
        struct ExceptionRecord {
            exception_code: u32,
            exception_flags: u32,
            exception_record: *mut ExceptionRecord,
            exception_address: *mut core::ffi::c_void,
            number_parameters: u32,
            exception_information: [usize; 15],
        }
        #[repr(C)]
        struct ExceptionPointers {
            exception_record: *mut ExceptionRecord,
            context_record: *mut core::ffi::c_void,
        }
        type VectoredHandler = unsafe extern "system" fn(*mut ExceptionPointers) -> i32;

        extern "system" {
            fn AddVectoredExceptionHandler(
                first: u32,
                handler: VectoredHandler,
            ) -> *mut core::ffi::c_void;
        }

        const EXCEPTION_ACCESS_VIOLATION: u32 = 0xC000_0005;
        const EXCEPTION_CONTINUE_SEARCH: i32 = 0;

        unsafe extern "system" fn crash_handler(info: *mut ExceptionPointers) -> i32 {
            if info.is_null() {
                return EXCEPTION_CONTINUE_SEARCH;
            }
            // SAFETY: `info` is non-null (checked above) and points to a valid
            // EXCEPTION_POINTERS struct provided by the Windows SEH runtime.
            let record = unsafe { (*info).exception_record };
            if record.is_null() {
                return EXCEPTION_CONTINUE_SEARCH;
            }
            // SAFETY: `record` is non-null (checked above) and points to a valid
            // EXCEPTION_RECORD struct provided by the Windows SEH runtime.
            let code = unsafe { (*record).exception_code };
            if code == EXCEPTION_ACCESS_VIOLATION {
                // SAFETY: `record` is a valid EXCEPTION_RECORD pointer (non-null, checked above).
                let addr = unsafe { (*record).exception_address as usize };
                // SAFETY: `record` is a valid EXCEPTION_RECORD; exception_information[0] holds the access type.
                let info0 = unsafe { (*record).exception_information[0] };
                // SAFETY: `record` is a valid EXCEPTION_RECORD; exception_information[1] holds the faulting address.
                let info1 = unsafe { (*record).exception_information[1] };
                let op = match info0 {
                    0 => "reading",
                    1 => "writing",
                    8 => "DEP violation at",
                    _ => "accessing",
                };
                eprintln!("\n=== STATUS_ACCESS_VIOLATION ===");
                eprintln!("Faulting instruction: 0x{addr:016x}");
                eprintln!("Operation: {op} address 0x{info1:016x}");
                eprintln!(
                    "Thread: {:?}",
                    std::thread::current().name().unwrap_or("unnamed")
                );
                eprintln!("\nBacktrace:");
                eprintln!("{}", std::backtrace::Backtrace::force_capture());
                eprintln!("=== END CRASH INFO ===\n");
            }
            EXCEPTION_CONTINUE_SEARCH
        }

        AddVectoredExceptionHandler(0, crash_handler);
    }
}

// ── Pre-main environment setup ───────────────────────────────────────────────

/// Set environment variables that must be in place before Tauri/GPU init.
pub(crate) fn setup_env() {
    // rustls: install ring crypto provider before any TLS connection.
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Failed to install rustls CryptoProvider");

    setup_gpu_env();
}

/// Set GPU-related env vars (Vulkan validation, Linux EGL).
/// Split out from `setup_env` so it can be tested without the one-shot
/// rustls provider install.
pub(crate) fn setup_gpu_env() {
    // ── Vulkan: disable validation layers in debug builds ────────────────
    if cfg!(debug_assertions) {
        set_if_absent("VK_LOADER_LAYERS_DISABLE", "VK_LAYER_KHRONOS_validation");
        set_if_absent("VK_INSTANCE_LAYERS", "");
        set_if_absent("WGPU_VALIDATION", "0");
        set_if_absent("WGPU_GPU_BASED_VALIDATION", "0");
    }

    // ── Linux: suppress noisy libEGL / DRI2 warnings ─────────────────────
    #[cfg(target_os = "linux")]
    {
        set_if_absent("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
        set_if_absent("EGL_LOG_LEVEL", "fatal");
    }
}

/// Set an environment variable only if it is not already defined.
fn set_if_absent(key: &str, value: &str) {
    if std::env::var(key).is_err() {
        std::env::set_var(key, value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Env-var tests run sequentially (Rust test harness uses threads, so
    // env vars are process-global).  We use unique var names to avoid
    // interference between tests.

    #[test]
    fn set_if_absent_sets_missing_var() {
        let key = "__SKILL_TEST_SET_IF_ABSENT_MISSING";
        std::env::remove_var(key);
        set_if_absent(key, "hello");
        assert_eq!(std::env::var(key).unwrap(), "hello");
        std::env::remove_var(key);
    }

    #[test]
    fn set_if_absent_preserves_existing_var() {
        let key = "__SKILL_TEST_SET_IF_ABSENT_EXISTING";
        std::env::set_var(key, "original");
        set_if_absent(key, "overwrite");
        assert_eq!(std::env::var(key).unwrap(), "original");
        std::env::remove_var(key);
    }

    #[test]
    fn setup_gpu_env_sets_vulkan_vars_in_debug() {
        // In debug builds (cfg!(debug_assertions) == true), Vulkan vars
        // should be populated.  In release builds this test is a no-op.
        let key = "WGPU_VALIDATION";
        let had_value = std::env::var(key).ok();
        std::env::remove_var(key);

        setup_gpu_env();

        if cfg!(debug_assertions) {
            assert_eq!(std::env::var(key).unwrap(), "0");
        }

        // Restore original state.
        match had_value {
            Some(v) => std::env::set_var(key, v),
            None => std::env::remove_var(key),
        }
    }
}
