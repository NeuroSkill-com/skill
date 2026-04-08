// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3 only.
//! Cross-platform calendar event fetching.
//!
//! | Platform | Backend                                              |
//! |----------|------------------------------------------------------|
//! | macOS    | Apple EventKit (`EKEventStore`) via Objective-C FFI |
//! | Linux    | iCal (`.ics`) files from XDG calendar app locations |
//! | Windows  | iCal (`.ics`) files from Outlook / Calendar paths   |
//!
//! # Quick start
//!
//! ```rust,ignore
//! use skill_calendar::{auth_status, fetch_events, request_access, AuthStatus};
//!
//! // Check / request permission first (macOS only; always granted elsewhere)
//! if auth_status() == AuthStatus::NotDetermined {
//!     request_access();
//! }
//!
//! // Fetch events for today
//! let now = std::time::SystemTime::now()
//!     .duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64;
//! let tomorrow = now + 86400;
//!
//! match fetch_events(now, tomorrow) {
//!     Ok(events) => {
//!         for ev in &events {
//!             println!("{} — {}", ev.title, ev.start_utc);
//!         }
//!     }
//!     Err(e) => eprintln!("calendar error: {e}"),
//! }
//! ```

mod ical; // compiled on all platforms; used by linux/windows and for tests
mod types;

#[cfg(target_os = "macos")]
mod macos;

#[cfg(target_os = "linux")]
mod linux;

#[cfg(target_os = "windows")]
mod windows;

pub use types::{AuthStatus, CalendarEvent};

// ── Public API ────────────────────────────────────────────────────────────────

/// Return the current calendar access authorisation status.
///
/// On Linux and Windows this always returns [`AuthStatus::Authorized`].
pub fn auth_status() -> AuthStatus {
    #[cfg(target_os = "macos")]
    return macos::auth_status();

    #[cfg(target_os = "linux")]
    return linux::auth_status();

    #[cfg(target_os = "windows")]
    return windows::auth_status();

    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    AuthStatus::Authorized
}

/// Prompt the user to grant calendar access (macOS only).
///
/// Blocks until the user responds or the 30-second timeout elapses.
/// Returns `true` if access was granted.  On Linux / Windows always
/// returns `true` without showing any dialog.
pub fn request_access() -> bool {
    #[cfg(target_os = "macos")]
    return macos::request_access();

    #[cfg(not(target_os = "macos"))]
    true
}

/// Fetch calendar events that overlap the `[start_utc, end_utc]` window.
///
/// Both timestamps are UTC unix seconds (inclusive).
///
/// Returns `Err` only on hard failures (access denied, parse error).
/// An empty `Vec` is returned when no events match the range.
pub fn fetch_events(start_utc: i64, end_utc: i64) -> anyhow::Result<Vec<CalendarEvent>> {
    #[cfg(target_os = "macos")]
    return macos::fetch_events(start_utc, end_utc);

    #[cfg(target_os = "linux")]
    return linux::fetch_events(start_utc, end_utc);

    #[cfg(target_os = "windows")]
    return windows::fetch_events(start_utc, end_utc);

    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    Ok(Vec::new())
}
