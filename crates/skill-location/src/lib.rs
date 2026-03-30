// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
//! Cross-platform location provider.
//!
//! | Platform | Backend                                                    |
//! |----------|------------------------------------------------------------|
//! | macOS    | Apple CoreLocation via Objective-C FFI (GPS/Wi-Fi/cell)    |
//! | Linux    | IP geolocation (`ipwho.is`)                                |
//! | Windows  | IP geolocation (`ipwho.is`)                                |
//!
//! On macOS, CoreLocation provides high-accuracy fixes (often < 100 m) using
//! the device's GPS, Wi-Fi, and Bluetooth hardware.  On other platforms the
//! crate falls back to IP-based geolocation which is typically accurate to the
//! city level (~1-25 km).
//!
//! # Quick start
//!
//! ```rust,ignore
//! use skill_location::{auth_status, fetch_location, request_access, LocationAuthStatus};
//!
//! // Check / request permission (macOS only; always Authorized elsewhere)
//! if auth_status() == LocationAuthStatus::NotDetermined {
//!     request_access(30.0);
//! }
//!
//! match fetch_location(10.0) {
//!     Ok(fix) => println!("{:.5}, {:.5} (source: {:?})", fix.latitude, fix.longitude, fix.source),
//!     Err(e) => eprintln!("location error: {e}"),
//! }
//! ```
//!
//! # Storing results
//!
//! The returned [`LocationFix`] can be converted into a
//! `skill_health::LocationSample` for storage in the health SQLite database
//! alongside other health data:
//!
//! ```rust,ignore
//! let fix = skill_location::fetch_location(10.0).unwrap();
//! let sample = skill_health::LocationSample {
//!     source_id: format!("{:?}", fix.source),
//!     timestamp: fix.timestamp as i64,
//!     latitude: fix.latitude,
//!     longitude: fix.longitude,
//!     altitude: fix.altitude,
//!     horizontal_accuracy: fix.horizontal_accuracy,
//!     vertical_accuracy: fix.vertical_accuracy,
//!     speed: fix.speed,
//!     course: fix.course,
//! };
//! ```

mod types;

#[cfg(target_os = "macos")]
mod macos;

// IP fallback is compiled on all non-macOS platforms.  On macOS it is still
// available as a fallback via `fetch_ip_location()` but not the primary path.
#[cfg(not(target_os = "macos"))]
mod ip_fallback;

#[cfg(target_os = "macos")]
#[path = "ip_fallback_stub.rs"]
mod ip_fallback_stub;

pub use types::*;

// ── Public API ────────────────────────────────────────────────────────────────

/// Return the current location authorisation status.
///
/// On Linux and Windows this always returns [`LocationAuthStatus::Authorized`]
/// because IP geolocation requires no OS-level permission.
pub fn auth_status() -> LocationAuthStatus {
    #[cfg(target_os = "macos")]
    return macos::auth_status();

    #[cfg(not(target_os = "macos"))]
    LocationAuthStatus::Authorized
}

/// Request location permission from the user (macOS only).
///
/// Blocks for up to `timeout_secs` seconds while the system dialog is shown.
/// Returns `true` if access was granted.  On Linux / Windows always returns
/// `true` without showing any dialog.
pub fn request_access(timeout_secs: f64) -> bool {
    #[cfg(target_os = "macos")]
    return macos::request_access(timeout_secs);

    #[cfg(not(target_os = "macos"))]
    {
        let _ = timeout_secs;
        true
    }
}

/// Fetch the current location.
///
/// * **macOS** — uses CoreLocation.  If CoreLocation fails or is denied, falls
///   back to IP geolocation automatically.
/// * **Linux / Windows** — uses the `ipwho.is` API.
///
/// `timeout_secs` controls the CoreLocation timeout on macOS.  On other
/// platforms the HTTP request uses its own 5-second timeout.
pub fn fetch_location(timeout_secs: f64) -> Result<LocationFix, LocationError> {
    #[cfg(target_os = "macos")]
    {
        match macos::fetch(timeout_secs) {
            Ok(fix) => return Ok(fix),
            Err(e) => {
                eprintln!("[location] CoreLocation failed ({e}), falling back to IP geolocation");
                // On macOS we still have ureq available via the ip_fallback_stub
                // which re-implements the same logic inline to avoid the cfg
                // dependency split.
                return ip_fallback_stub::fetch_ip_location();
            }
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = timeout_secs;
        ip_fallback::fetch_ip_location()
    }
}

/// Fetch location using only the IP geolocation API (all platforms).
///
/// This is useful when you want the city/country/timezone metadata that
/// CoreLocation does not provide.
pub fn fetch_ip_location() -> Result<LocationFix, LocationError> {
    #[cfg(not(target_os = "macos"))]
    return ip_fallback::fetch_ip_location();

    #[cfg(target_os = "macos")]
    ip_fallback_stub::fetch_ip_location()
}
