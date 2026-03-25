// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
use serde::{Deserialize, Serialize};

/// A single calendar event normalised to UTC unix-second timestamps.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalendarEvent {
    /// Unique identifier (UID from iCal / `eventIdentifier` from EventKit).
    pub id: String,
    /// Event title / summary.
    pub title: String,
    /// Start time as UTC unix seconds.
    pub start_utc: i64,
    /// End time as UTC unix seconds.
    pub end_utc: i64,
    /// `true` for all-day events (no time component).
    pub all_day: bool,
    /// Optional location string.
    pub location: Option<String>,
    /// Optional description / notes.
    pub notes: Option<String>,
    /// Name of the calendar (account / list) the event belongs to.
    pub calendar: Option<String>,
    /// `"confirmed"`, `"tentative"`, or `"cancelled"`.
    pub status: String,
    /// RRULE recurrence string as-is from iCal (not expanded).
    pub recurrence: Option<String>,
}

/// Calendar access authorisation status (mirrors EventKit on macOS).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthStatus {
    /// The user has not yet been asked.
    NotDetermined,
    /// Access was granted.
    Authorized,
    /// Access was explicitly denied.
    Denied,
    /// Restricted by MDM / parental controls.
    Restricted,
}
