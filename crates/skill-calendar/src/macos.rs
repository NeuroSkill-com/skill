// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! macOS EventKit calendar bridge.

use crate::types::{AuthStatus, CalendarEvent};

extern "C" {
    fn skill_calendar_auth_status() -> i32;
    fn skill_calendar_request_access() -> i32;
    fn skill_calendar_fetch_events(start_utc: i64, end_utc: i64, out_len: *mut u32) -> *mut u8;
    fn free(ptr: *mut std::ffi::c_void);
}

/// SAFETY: pointer was returned by `malloc` in the ObjC layer.
unsafe fn c_free(ptr: *mut u8) {
    // SAFETY: `ptr` was allocated by `malloc` in the Objective-C FFI.
    unsafe { free(ptr as *mut std::ffi::c_void) };
}

pub fn auth_status() -> AuthStatus {
    // SAFETY: no data is passed; the C function is pure read-only.
    match unsafe { skill_calendar_auth_status() } {
        1 => AuthStatus::Authorized,
        2 => AuthStatus::Denied,
        3 => AuthStatus::Restricted,
        _ => AuthStatus::NotDetermined,
    }
}

pub fn request_access() -> bool {
    // SAFETY: calls into ObjC EventKit which prompts the user via a system
    // dialog; no Rust-owned data is transferred.
    unsafe { skill_calendar_request_access() == 1 }
}

pub fn fetch_events(start_utc: i64, end_utc: i64) -> Result<Vec<CalendarEvent>, String> {
    // SAFETY: `skill_calendar_fetch_events` returns a malloc'd UTF-8 JSON
    // buffer.  We copy the bytes into a Rust String and immediately free the
    // C buffer, so there are no dangling references.
    let json_str = unsafe {
        let mut len: u32 = 0;
        let ptr = skill_calendar_fetch_events(start_utc, end_utc, &mut len);
        if ptr.is_null() {
            return Err("EventKit returned null (allocation failure)".into());
        }
        let slice = std::slice::from_raw_parts(ptr, len as usize);
        let s = String::from_utf8_lossy(slice).into_owned();
        c_free(ptr);
        s
    };

    // The ObjC layer returns either a JSON array (success) or a JSON object
    // with an "error" key (access denied / allocation failure).
    // Parse the value first to distinguish the two cases robustly — checking
    // for the substring `"error"` would false-positive on event titles like
    // "error in production".
    match serde_json::from_str::<serde_json::Value>(&json_str) {
        Ok(serde_json::Value::Array(_)) => {
            serde_json::from_str::<Vec<CalendarEvent>>(&json_str).map_err(|e| format!("JSON parse error: {e}"))
        }
        Ok(serde_json::Value::Object(obj)) => {
            let msg = obj
                .get("error")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown error from EventKit");
            Err(msg.to_owned())
        }
        Ok(_) => Ok(Vec::new()),
        Err(e) => Err(format!("JSON parse error: {e}")),
    }
}
