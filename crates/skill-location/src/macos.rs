// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! macOS CoreLocation backend.

use crate::types::{LocationAuthStatus, LocationError, LocationFix, LocationSource};

// ── FFI declarations ─────────────────────────────────────────────────────────

#[repr(C)]
struct SkillLocationResult {
    ok: i32,
    latitude: f64,
    longitude: f64,
    altitude: f64,
    horizontal_accuracy: f64,
    vertical_accuracy: f64,
    speed: f64,
    course: f64,
    timestamp: f64,
    auth_status: i32,
    error: [u8; 256],
}

extern "C" {
    fn skill_location_auth_status() -> i32;
    fn skill_location_request_access(timeout_secs: f64) -> i32;
    fn skill_location_fetch(timeout_secs: f64, out: *mut SkillLocationResult);
}

// ── Public API ───────────────────────────────────────────────────────────────

pub fn auth_status() -> LocationAuthStatus {
    // SAFETY: `skill_location_auth_status` is a plain C function with no
    // pointers or mutable state — it allocates a temporary CLLocationManager
    // on the main thread and returns an integer enum value.
    let raw = unsafe { skill_location_auth_status() };
    match raw {
        0 => LocationAuthStatus::NotDetermined,
        1 => LocationAuthStatus::Restricted,
        2 => LocationAuthStatus::Denied,
        _ => LocationAuthStatus::Authorized,
    }
}

pub fn request_access(timeout_secs: f64) -> bool {
    // SAFETY: `skill_location_request_access` is a plain C function that
    // takes a scalar timeout and returns 0/1.  It dispatches to the main
    // thread internally, so the call is thread-safe.
    unsafe { skill_location_request_access(timeout_secs) == 1 }
}

pub fn fetch(timeout_secs: f64) -> Result<LocationFix, LocationError> {
    let mut result = SkillLocationResult {
        ok: 0,
        latitude: 0.0,
        longitude: 0.0,
        altitude: f64::NAN,
        horizontal_accuracy: -1.0,
        vertical_accuracy: -1.0,
        speed: -1.0,
        course: -1.0,
        timestamp: 0.0,
        auth_status: 0,
        error: [0u8; 256],
    };

    // SAFETY: `skill_location_fetch` writes into `result` through a valid
    // pointer.  The struct is stack-allocated and exclusively owned by this
    // call, and the C function initialises every field before returning.
    unsafe { skill_location_fetch(timeout_secs, &mut result) };

    if result.ok == 1 {
        let opt = |v: f64| if v.is_nan() || v < 0.0 { None } else { Some(v) };
        Ok(LocationFix {
            latitude: result.latitude,
            longitude: result.longitude,
            altitude: if result.altitude.is_nan() { None } else { Some(result.altitude) },
            horizontal_accuracy: opt(result.horizontal_accuracy),
            vertical_accuracy: opt(result.vertical_accuracy),
            speed: opt(result.speed),
            course: opt(result.course),
            timestamp: result.timestamp,
            country: None,
            region: None,
            city: None,
            timezone: None,
            source: LocationSource::CoreLocation,
        })
    } else {
        let err_msg = {
            let len = result.error.iter().position(|&b| b == 0).unwrap_or(result.error.len());
            String::from_utf8_lossy(&result.error[..len]).to_string()
        };
        if err_msg.contains("timed out") {
            Err(LocationError::Timeout)
        } else if err_msg.contains("not authorized") {
            Err(LocationError::NotAuthorized(err_msg))
        } else {
            Err(LocationError::Failed(err_msg))
        }
    }
}
