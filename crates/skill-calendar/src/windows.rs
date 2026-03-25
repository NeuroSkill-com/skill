// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Windows calendar event provider.
//!
//! Scans common Windows locations for `.ics` files:
//! * `%APPDATA%\Microsoft\Outlook\`          — Outlook exports / subscriptions
//! * `%LOCALAPPDATA%\Microsoft\Outlook\`     — cached/imported calendars
//! * `%USERPROFILE%\Documents\`              — user-saved exports
//! * UWP package data dirs for Windows Calendar / Outlook for Windows
//!
//! All found `.ics` files are parsed with the shared iCal parser.

use std::path::PathBuf;

use crate::ical::parse_ical;
use crate::types::{AuthStatus, CalendarEvent};

/// On Windows access is always "authorized" (no OS-level permission gate for
/// desktop apps reading local `.ics` files).
pub fn auth_status() -> AuthStatus {
    AuthStatus::Authorized
}

/// No-op on Windows.
#[allow(dead_code)]
pub fn request_access() -> bool {
    true
}

pub fn fetch_events(start_utc: i64, end_utc: i64) -> Result<Vec<CalendarEvent>, String> {
    let search_roots = build_search_roots();

    let mut events: Vec<CalendarEvent> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

    for root in &search_roots {
        if root.is_dir() {
            walk_ics(root, &mut events, &mut seen, start_utc, end_utc, 0);
        } else if root.is_file() {
            parse_ics_file(root, &mut events, &mut seen, start_utc, end_utc);
        }
    }

    Ok(events)
}

fn build_search_roots() -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();

    // %APPDATA% (C:\Users\<name>\AppData\Roaming)
    if let Ok(appdata) = std::env::var("APPDATA") {
        let base = PathBuf::from(&appdata);
        roots.push(base.join("Microsoft").join("Outlook"));
    }

    // %LOCALAPPDATA% (C:\Users\<name>\AppData\Local)
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        let base = PathBuf::from(&local);
        roots.push(base.join("Microsoft").join("Outlook"));

        // UWP package data — glob for windowscommunicationsapps and OutlookForWindows
        let packages = base.join("Packages");
        if let Ok(entries) = std::fs::read_dir(&packages) {
            for entry in entries.filter_map(std::result::Result::ok) {
                let name = entry.file_name().to_string_lossy().to_lowercase();
                if name.contains("windowscommunicationsapps")
                    || name.contains("outlookforwindows")
                    || name.contains("microsoft.outlook")
                {
                    let local_state = entry.path().join("LocalState");
                    if local_state.is_dir() {
                        roots.push(local_state);
                    }
                }
            }
        }
    }

    // %USERPROFILE%\Documents\
    if let Ok(profile) = std::env::var("USERPROFILE") {
        let base = PathBuf::from(&profile);
        roots.push(base.join("Documents"));
        roots.push(base.join("Calendars"));
        roots.push(base.join("Calendar"));
    }

    roots
}

fn walk_ics(
    dir: &std::path::Path,
    events: &mut Vec<CalendarEvent>,
    seen: &mut std::collections::HashSet<String>,
    start_utc: i64,
    end_utc: i64,
    depth: usize,
) {
    if depth > 5 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.filter_map(std::result::Result::ok) {
        let path = entry.path();
        if path.is_dir() {
            walk_ics(&path, events, seen, start_utc, end_utc, depth + 1);
        } else if path.extension().and_then(|e| e.to_str()) == Some("ics") {
            parse_ics_file(&path, events, seen, start_utc, end_utc);
        }
    }
}

fn parse_ics_file(
    path: &std::path::Path,
    events: &mut Vec<CalendarEvent>,
    seen: &mut std::collections::HashSet<String>,
    start_utc: i64,
    end_utc: i64,
) {
    let Ok(content) = std::fs::read_to_string(path) else {
        return;
    };

    let cal_name: Option<String> = path
        .file_stem()
        .and_then(|n| n.to_str())
        .map(|s| s.replace(['-', '_'], " "));

    let parsed = parse_ical(&content, start_utc, end_utc);

    for mut ev in parsed {
        if ev.calendar.is_none() {
            ev.calendar.clone_from(&cal_name);
        }
        let key = if ev.id.is_empty() {
            format!("{}\x00{}", ev.start_utc, ev.title)
        } else {
            ev.id.clone()
        };
        if seen.insert(key) {
            events.push(ev);
        }
    }
}
