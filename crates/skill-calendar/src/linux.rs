// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Linux calendar event provider.
//!
//! Scans standard XDG locations for `.ics` files written by:
//! * GNOME Calendar (`~/.local/share/gnome-calendar/`)
//! * Evolution (`~/.local/share/evolution/calendar/`)
//! * Thunderbird / Lightning (`.local/share/` subtree)
//! * KOrganizer (`~/.local/share/korganizer/` and `~/.kde/share/apps/korganizer/`)
//! * Generic user calendars (`~/Calendars/`, `~/.calendars/`)
//!
//! All found `.ics` files are parsed with the shared iCal parser.

use std::path::PathBuf;

use crate::ical::parse_ical;
use crate::types::{AuthStatus, CalendarEvent};

/// On Linux access is always "authorized" (no OS-level permission gate).
pub fn auth_status() -> AuthStatus {
    AuthStatus::Authorized
}

/// No-op on Linux — access is always granted.
#[allow(dead_code)]
pub fn request_access() -> bool {
    true
}

pub fn fetch_events(start_utc: i64, end_utc: i64) -> Result<Vec<CalendarEvent>, String> {
    let Some(home) = dirs_home() else {
        return Ok(Vec::new());
    };

    let mut search_roots: Vec<PathBuf> = vec![
        home.join(".local/share/gnome-calendar"),
        home.join(".local/share/evolution/calendar"),
        home.join(".local/share/korganizer"),
        home.join(".kde/share/apps/korganizer"),
        home.join(".kde4/share/apps/korganizer"),
        home.join(".local/share/akonadi"),
        home.join("Calendars"),
        home.join(".calendars"),
        home.join("Calendar"),
    ];

    // Thunderbird / Lightning: only scan the known calendar-data subdirectory
    // inside each profile — NOT the whole profile tree (which contains
    // potentially gigabytes of mail data and hundreds of thousands of files).
    for tb_root in [home.join(".thunderbird"), home.join(".mozilla-thunderbird")] {
        if let Ok(profiles) = std::fs::read_dir(&tb_root) {
            for profile in profiles.filter_map(std::result::Result::ok) {
                let cal = profile.path().join("calendar-data");
                if cal.is_dir() {
                    search_roots.push(cal);
                }
            }
        }
    }

    let mut events: Vec<CalendarEvent> = Vec::new();
    let mut seen_ids: std::collections::HashSet<String> = std::collections::HashSet::new();

    for root in &search_roots {
        if root.is_dir() {
            walk_ics(root, &mut events, &mut seen_ids, start_utc, end_utc, 0);
        }
    }

    // De-duplicate by id, keeping first occurrence
    Ok(events)
}

/// Recursively find and parse `.ics` files up to `max_depth` levels deep.
fn walk_ics(
    dir: &std::path::Path,
    events: &mut Vec<CalendarEvent>,
    seen: &mut std::collections::HashSet<String>,
    start_utc: i64,
    end_utc: i64,
    depth: usize,
) {
    if depth > 6 {
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

    // Derive calendar name from directory name (heuristic)
    let cal_name: Option<String> = path
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .map(|s| s.replace(['-', '_'], " "));

    let parsed = parse_ical(&content, start_utc, end_utc);

    for mut ev in parsed {
        if ev.calendar.is_none() {
            ev.calendar.clone_from(&cal_name);
        }
        // De-duplicate: use UID when present, otherwise fall back to
        // start_utc+title so anonymous events still deduplicate correctly.
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

fn dirs_home() -> Option<PathBuf> {
    std::env::var("HOME").ok().map(PathBuf::from)
}
