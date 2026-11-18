// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
//! Wall-clock time, time zones and date parsing for the workspace.
//!
//! The rest of the workspace asks for times here, in its own terms (Unix
//! seconds, `YYYY-MM-DD` dates, format strings), and never names the date
//! library behind it — jiff today — so swapping that library is a change to
//! this crate alone. Format strings are `strftime`-style.
//!
//! "Local" is the system time zone, honouring `TZ` as the OS does.

use std::fmt;

use jiff::fmt::strtime;
use jiff::tz::{Offset, TimeZone};
use jiff::{civil, Span, Timestamp, ZonedDateTime};

/// Seconds since the Unix epoch, now.
pub fn now_unix() -> i64 {
    Timestamp::now().as_second()
}

/// The local time zone's current UTC offset in seconds (e.g. -28800 for UTC-8).
pub fn local_offset_secs() -> i32 {
    ZonedDateTime::now().offset().seconds()
}

/// Now in UTC as RFC 3339 with an explicit `+00:00` and the clock's full
/// precision, e.g. `2026-11-17T09:30:00.123456789+00:00`.
pub fn now_rfc3339_utc() -> String {
    Timestamp::now().display_with_offset(Offset::UTC).to_string()
}

/// Now on the local wall clock, formatted with `fmt`.
pub fn local_now_format(fmt: &str) -> String {
    LocalNow::now().format(fmt)
}

/// A moment on the local wall clock, for code that asks several questions of
/// the same "now" (the hour, the start of the day, a formatted stamp).
#[derive(Clone, Debug)]
pub struct LocalNow(ZonedDateTime);

impl LocalNow {
    pub fn now() -> Self {
        Self(ZonedDateTime::now())
    }

    /// Seconds since the Unix epoch.
    pub fn unix(&self) -> i64 {
        self.0.timestamp().as_second()
    }

    /// The UTC offset in force at this moment, in seconds.
    pub fn offset_secs(&self) -> i32 {
        self.0.offset().seconds()
    }

    /// Hour of the day, 0–23.
    pub fn hour(&self) -> u8 {
        self.0.hour() as u8
    }

    /// Day of the week, Monday = 1 … Sunday = 7.
    pub fn weekday_from_monday(&self) -> u8 {
        self.0.weekday().to_monday_one_offset() as u8
    }

    /// ISO 8601 week number, 1–53.
    pub fn iso_week(&self) -> u8 {
        self.0.date().iso_week_date().week() as u8
    }

    /// Day of the year, 1–366.
    pub fn day_of_year(&self) -> u16 {
        self.0.day_of_year() as u16
    }

    /// Unix seconds of the first instant of today (midnight, or the end of
    /// a DST gap that swallows it).
    pub fn start_of_day_unix(&self) -> Option<i64> {
        self.0.start_of_day().ok().map(|t| t.timestamp().as_second())
    }

    /// [`Self::start_of_day_unix`] for the calendar day `days` days back.
    pub fn start_of_day_days_ago_unix(&self, days: i64) -> Option<i64> {
        self.0
            .checked_sub(Span::new().try_days(days).ok()?)
            .and_then(|t| t.start_of_day())
            .ok()
            .map(|t| t.timestamp().as_second())
    }

    /// Unix seconds of the next local midnight (the first instant of
    /// tomorrow); now itself if that cannot be represented.
    pub fn next_midnight_unix(&self) -> i64 {
        self.0
            .tomorrow()
            .and_then(|t| t.start_of_day())
            .map_or_else(|_| self.unix(), |t| t.timestamp().as_second())
    }

    /// Formatted with a `strftime`-style `fmt`.
    pub fn format(&self, fmt: &str) -> String {
        self.0.strftime(fmt).to_string()
    }

    /// RFC 3339 in UTC to the millisecond, `Z`-suffixed:
    /// `2026-11-17T09:30:00.123Z`.
    pub fn rfc3339_utc_millis(&self) -> String {
        format!("{:.3}", self.0.timestamp())
    }

    /// RFC 3339 on the local clock to the millisecond, with its offset:
    /// `2026-11-17T10:30:00.123+01:00`.
    pub fn rfc3339_local_millis(&self) -> String {
        self.format("%Y-%m-%dT%H:%M:%S%.3f%:z")
    }
}

/// Unix seconds of a local wall-clock time; `None` if the date or time is
/// invalid, or the clock reading is ambiguous or skipped (a DST change).
pub fn local_civil_to_unix(year: i16, month: i8, day: i8, hour: i8, minute: i8, second: i8) -> Option<i64> {
    civil_to_unix_in(&TimeZone::system(), year, month, day, hour, minute, second)
}

fn civil_to_unix_in(tz: &TimeZone, year: i16, month: i8, day: i8, hour: i8, minute: i8, second: i8) -> Option<i64> {
    let dt = civil::DateTime::new(year, month, day, hour, minute, second, 0).ok()?;
    tz.to_ambiguous_timestamp(dt).unambiguous().ok().map(|t| t.as_second())
}

/// Unix seconds of an ISO 8601 date-time: RFC 3339 with an offset or `Z`
/// (`2026-03-15T23:45:00+02:00`), or with no zone at all
/// (`2026-03-15T21:45:00`, optionally with fractional seconds), read as UTC.
pub fn parse_datetime_unix(s: &str) -> Option<i64> {
    if let Ok(ts) = s.parse::<Timestamp>() {
        return Some(ts.as_second());
    }
    strtime::parse("%Y-%m-%dT%H:%M:%S%.f", s)
        .and_then(|t| t.to_datetime())
        .and_then(|dt| dt.to_zoned_datetime(TimeZone::UTC))
        .ok()
        .map(|t| t.timestamp().as_second())
}

/// Unix seconds of midnight UTC on a `YYYY-MM-DD` date.
pub fn parse_date_unix(s: &str) -> Option<i64> {
    Date::parse_ymd(s)?.midnight_utc_unix()
}

/// A calendar date with no time or zone.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Date(civil::Date);

impl Date {
    /// `YYYY-MM-DD`, exactly.
    pub fn parse_ymd(s: &str) -> Option<Self> {
        strtime::parse("%Y-%m-%d", s).and_then(|t| t.to_date()).ok().map(Self)
    }

    /// Today's date in UTC.
    pub fn today_utc() -> Self {
        Self(Timestamp::now().to_zoned_datetime(TimeZone::UTC).date())
    }

    /// `days` calendar days earlier, clamped to the earliest representable date.
    pub fn minus_days(self, days: i32) -> Self {
        Self(self.0.saturating_sub(Span::new().days(days)))
    }

    /// Unix seconds of midnight UTC on this date.
    pub fn midnight_utc_unix(self) -> Option<i64> {
        self.0
            .to_zoned_datetime(TimeZone::UTC)
            .ok()
            .map(|t| t.timestamp().as_second())
    }
}

/// `YYYY-MM-DD`.
impl fmt::Display for Date {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn datetime_with_offset_or_z() {
        assert_eq!(parse_datetime_unix("2026-03-15T12:00:00+00:00"), Some(1_773_576_000));
        assert_eq!(parse_datetime_unix("2026-03-15T14:00:00+02:00"), Some(1_773_576_000));
        assert_eq!(parse_datetime_unix("2026-03-15T12:00:00Z"), Some(1_773_576_000));
        assert_eq!(parse_datetime_unix("2026-03-15T12:00:00.250Z"), Some(1_773_576_000));
    }

    #[test]
    fn datetime_without_zone_is_utc() {
        assert_eq!(parse_datetime_unix("2026-03-15T12:00:00"), Some(1_773_576_000));
        assert_eq!(parse_datetime_unix("2026-03-15T12:00:00.123"), Some(1_773_576_000));
    }

    #[test]
    fn bad_input_is_none() {
        assert_eq!(parse_datetime_unix("not-a-date"), None);
        assert_eq!(parse_datetime_unix("2026-03-15T25:00:00"), None);
        assert_eq!(parse_date_unix("invalid"), None);
        assert_eq!(parse_date_unix("2026-02-30"), None);
        assert_eq!(Date::parse_ymd("2026-03-15T00:00:00"), None);
    }

    #[test]
    fn dates() {
        assert_eq!(parse_date_unix("2026-03-15"), Some(1_773_532_800));
        let d = Date::parse_ymd("2026-03-01").unwrap();
        assert_eq!(d.to_string(), "2026-03-01");
        assert_eq!(d.minus_days(30).to_string(), "2026-01-30");
    }

    #[test]
    fn civil_to_unix_rejects_invalid_and_dst_changes() {
        let ny = TimeZone::get("America/New_York").unwrap();
        assert_eq!(civil_to_unix_in(&ny, 2026, 4, 25, 18, 41, 13), Some(1_777_156_873));
        assert_eq!(civil_to_unix_in(&ny, 2026, 2, 30, 0, 0, 0), None);
        // 02:30 is skipped on 2026-03-08 and happens twice on 2026-11-01.
        assert_eq!(civil_to_unix_in(&ny, 2026, 3, 8, 2, 30, 0), None);
        assert_eq!(civil_to_unix_in(&ny, 2026, 11, 1, 1, 30, 0), None);
    }

    #[test]
    fn local_now_is_consistent() {
        let now = LocalNow::now();
        assert!((now.unix() - now_unix()).abs() <= 1);
        assert!(now.hour() < 24);
        assert!((1..=7).contains(&now.weekday_from_monday()));
        assert!((1..=53).contains(&now.iso_week()));
        let start = now.start_of_day_unix().unwrap();
        assert!(start <= now.unix() && now.unix() - start < 25 * 3600);
        assert!(now.next_midnight_unix() > now.unix());
        assert!(now.start_of_day_days_ago_unix(7).unwrap() < start);
        assert!(now.rfc3339_utc_millis().ends_with('Z'));
        assert_eq!(now.format("%Y%m%d-%H%M%S").len(), 15);
        assert!(now_rfc3339_utc().contains("+00:00"));
    }
}
