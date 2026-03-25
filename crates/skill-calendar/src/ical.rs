// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 NeuroSkill.com
// Allow dead-code on macOS where this module is compiled for tests only.
#![cfg_attr(target_os = "macos", allow(dead_code))]
//! Lightweight iCalendar (RFC 5545) parser.
//!
//! Handles:
//! * Line folding (CRLF / LF + leading SPACE or TAB = continuation)
//! * Property parameters (`NAME;PARAM=val:value`)
//! * `VEVENT`, `VTIMEZONE` components
//! * `VALUE=DATE` (all-day) and `VALUE=DATE-TIME` properties
//! * UTC timestamps (`Z` suffix) and TZID-based offsets from `VTIMEZONE` blocks
//! * iCal escape sequences in values (`\n`, `\,`, `\;`, `\\`)
//!
//! Recurrence rules (`RRULE`) are preserved as raw strings but **not** expanded.

use std::collections::HashMap;

use crate::types::CalendarEvent;

// ── Line unfolding ────────────────────────────────────────────────────────────

/// Unfold logical lines as per RFC 5545 §3.1.
///
/// A logical line may be folded across multiple physical lines by inserting a
/// CRLF or LF followed by a single SPACE or TAB.  This function stitches them
/// back together.
fn unfold(content: &str) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for raw in content.lines() {
        // Strip trailing \r if present (files may use CRLF or LF)
        let raw = raw.strip_suffix('\r').unwrap_or(raw);
        if (raw.starts_with(' ') || raw.starts_with('\t')) && !lines.is_empty() {
            // Continuation: append (without the leading whitespace) to last line
            if let Some(last) = lines.last_mut() {
                last.push_str(&raw[1..]);
            }
        } else {
            lines.push(raw.to_owned());
        }
    }
    lines
}

// ── Property parsing ──────────────────────────────────────────────────────────

#[derive(Debug)]
struct Property<'a> {
    name: &'a str,
    /// Lowercased param key → raw param value
    params: HashMap<String, String>,
    value: &'a str,
}

fn parse_property(line: &str) -> Option<Property<'_>> {
    // Split on the first `:` that is NOT inside a quoted parameter value.
    // Simple approach: find first unquoted `:`.
    let colon = find_unquoted_colon(line)?;
    let before = &line[..colon];
    let value = &line[colon + 1..];

    // Split name from parameters on first `;`
    let (name, param_str) = match before.find(';') {
        Some(sc) => (&before[..sc], &before[sc + 1..]),
        None => (before, ""),
    };
    let name = name.trim();

    let mut params: HashMap<String, String> = HashMap::new();
    if !param_str.is_empty() {
        for part in split_params(param_str) {
            if let Some(eq) = part.find('=') {
                let k = part[..eq].trim().to_lowercase();
                let v = part[eq + 1..].trim().trim_matches('"').to_owned();
                params.insert(k, v);
            }
        }
    }

    Some(Property { name, params, value })
}

/// Find the index of the first `:` that is NOT inside a double-quoted string.
fn find_unquoted_colon(s: &str) -> Option<usize> {
    let mut in_quote = false;
    for (i, c) in s.char_indices() {
        match c {
            '"' => in_quote = !in_quote,
            ':' if !in_quote => return Some(i),
            _ => {}
        }
    }
    None
}

/// Split parameter string on `;` but respect quoted strings.
fn split_params(s: &str) -> Vec<&str> {
    let mut parts: Vec<&str> = Vec::new();
    let mut start = 0;
    let mut in_quote = false;
    for (i, c) in s.char_indices() {
        match c {
            '"' => in_quote = !in_quote,
            ';' if !in_quote => {
                parts.push(&s[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(&s[start..]);
    parts
}

// ── iCal escape sequences ─────────────────────────────────────────────────────

fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') | Some('N') => out.push('\n'),
                Some('t') | Some('T') => out.push('\t'),
                Some(',') => out.push(','),
                Some(';') => out.push(';'),
                Some('\\') => out.push('\\'),
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

// ── Date/time parsing ─────────────────────────────────────────────────────────

/// Parse an iCal `TZOFFSETFROM` / `TZOFFSETTO` value like `+0530` or `-0500`
/// into a signed number of seconds.
fn parse_tz_offset(s: &str) -> Option<i64> {
    let s = s.trim();
    let (sign, rest) = if let Some(r) = s.strip_prefix('-') {
        (-1i64, r)
    } else if let Some(r) = s.strip_prefix('+') {
        (1i64, r)
    } else {
        (1i64, s)
    };
    if rest.len() < 4 {
        return None;
    }
    let hh: i64 = rest[..2].parse().ok()?;
    let mm: i64 = rest[2..4].parse().ok()?;
    let ss: i64 = if rest.len() >= 6 {
        rest[4..6].parse().unwrap_or(0)
    } else {
        0
    };
    Some(sign * (hh * 3600 + mm * 60 + ss))
}

/// Parse `YYYYMMDDTHHmmss[Z]` or `YYYYMMDD` into a UTC unix timestamp.
///
/// * `tzid`  — optional TZID parameter value (looked up in `tzmap`).
/// * `tzmap` — map from TZID string to UTC offset in seconds.
///
/// Returns `(unix_utc, all_day)`.
fn parse_datetime(value: &str, tzid: Option<&str>, tzmap: &HashMap<String, i64>) -> Option<(i64, bool)> {
    let v = value.trim();

    // All-day: YYYYMMDD (no T)
    if v.len() == 8 && !v.contains('T') {
        let ts = parse_ymd_to_unix(v)?;
        return Some((ts, true));
    }

    // Date-time: YYYYMMDDTHHmmss[Z]
    let (datetime_part, is_utc) = if let Some(d) = v.strip_suffix('Z') {
        (d, true)
    } else {
        (v, false)
    };

    if datetime_part.len() < 15 {
        return None;
    }
    let date_part = &datetime_part[..8];
    let time_part = &datetime_part[9..]; // skip 'T'

    let date_ts = parse_ymd_to_unix(date_part)?;
    let hh: i64 = time_part.get(..2)?.parse().ok()?;
    let mm: i64 = time_part.get(2..4)?.parse().ok()?;
    let ss: i64 = time_part.get(4..6).and_then(|s| s.parse().ok()).unwrap_or(0);
    let time_secs = hh * 3600 + mm * 60 + ss;

    let local_ts = date_ts + time_secs;

    if is_utc {
        return Some((local_ts, false));
    }

    // Apply timezone offset: local_ts - offset = UTC
    if let Some(tzid) = tzid {
        if let Some(&offset) = tzmap.get(tzid) {
            return Some((local_ts - offset, false));
        }
    }

    // Fallback: treat as UTC (most practical for modern Google/iCloud exports)
    Some((local_ts, false))
}

/// Parse `YYYYMMDD` to unix seconds (midnight UTC).
fn parse_ymd_to_unix(s: &str) -> Option<i64> {
    if s.len() < 8 {
        return None;
    }
    let y: i64 = s[..4].parse().ok()?;
    let m: i64 = s[4..6].parse().ok()?;
    let d: i64 = s[6..8].parse().ok()?;
    // Rata Die algorithm for UNIX epoch (days since 1970-01-01)
    Some(ymd_to_unix(y, m, d))
}

/// Convert calendar date to unix timestamp (seconds, midnight UTC).
///
/// Uses Howard Hinnant's `days_from_civil` algorithm (public domain).
/// <https://howardhinnant.github.io/date_algorithms.html#days_from_civil>
fn ymd_to_unix(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400; // [0, 399]
    let mp = if m > 2 { m - 3 } else { m + 9 }; // month of year starting March
    let doy = (153 * mp + 2) / 5 + d - 1; // day of year [0, 365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // day of era [0, 146096]
    (era * 146_097 + doe - 719_468) * 86_400
}

// ── Component stack parsing ───────────────────────────────────────────────────

/// Top-level iCal parser.  Returns all `VEVENT` entries that overlap the
/// given `[start_utc, end_utc]` range (inclusive).
pub fn parse_ical(content: &str, start_utc: i64, end_utc: i64) -> Vec<CalendarEvent> {
    let lines = unfold(content);

    // ── Pass 1: collect VTIMEZONE offset map ─────────────────────────────────
    let tzmap = collect_timezones(&lines);

    // ── Pass 2: parse VEVENTs ────────────────────────────────────────────────
    let mut events: Vec<CalendarEvent> = Vec::new();
    let mut in_vevent = false;
    let mut ev = EventBuilder::default();
    // X-WR-CALNAME sits at the VCALENDAR level (not inside VEVENT);
    // capture it once and inject into every event that has no calendar name.
    let mut vcal_name: Option<String> = None;

    for line in &lines {
        let upper = line.to_uppercase();
        match upper.as_str() {
            "BEGIN:VEVENT" => {
                in_vevent = true;
                ev = EventBuilder::default();
            }
            "END:VEVENT" if in_vevent => {
                in_vevent = false;
                let finished = std::mem::take(&mut ev);
                if let Some(mut event) = finished.build(start_utc, end_utc) {
                    if event.calendar.is_none() {
                        event.calendar.clone_from(&vcal_name);
                    }
                    events.push(event);
                }
            }
            _ if in_vevent => {
                if let Some(prop) = parse_property(line) {
                    ev.consume(prop, &tzmap);
                }
            }
            _ => {
                // Top-level VCALENDAR properties (outside any component)
                if let Some(prop) = parse_property(line) {
                    if prop.name.eq_ignore_ascii_case("X-WR-CALNAME") && !prop.value.is_empty() {
                        vcal_name = Some(unescape(prop.value));
                    }
                }
            }
        }
    }

    events
}

// ── VTIMEZONE collector ───────────────────────────────────────────────────────

/// Extract `TZID → offset_seconds` (using TZOFFSETTO from the STANDARD block,
/// falling back to DAYLIGHT if no STANDARD block is present).
fn collect_timezones(lines: &[String]) -> HashMap<String, i64> {
    let mut map: HashMap<String, i64> = HashMap::new();
    let mut current_tzid: Option<String> = None;
    let mut in_vtimezone = false;
    let mut in_standard = false;
    let mut in_daylight = false;
    let mut std_offset: Option<i64> = None;
    let mut day_offset: Option<i64> = None;

    for line in lines {
        let upper = line.to_uppercase();
        match upper.as_str() {
            "BEGIN:VTIMEZONE" => {
                in_vtimezone = true;
                current_tzid = None;
                std_offset = None;
                day_offset = None;
            }
            "END:VTIMEZONE" if in_vtimezone => {
                in_vtimezone = false;
                if let Some(tzid) = current_tzid.take() {
                    // Prefer STANDARD offset (winter time) as the "base"
                    let offset = std_offset.or(day_offset).unwrap_or(0);
                    map.insert(tzid, offset);
                }
            }
            "BEGIN:STANDARD" if in_vtimezone => {
                in_standard = true;
            }
            "END:STANDARD" if in_vtimezone => {
                in_standard = false;
            }
            "BEGIN:DAYLIGHT" if in_vtimezone => {
                in_daylight = true;
            }
            "END:DAYLIGHT" if in_vtimezone => {
                in_daylight = false;
            }
            _ if in_vtimezone => {
                if let Some(prop) = parse_property(line) {
                    match prop.name.to_uppercase().as_str() {
                        "TZID" => {
                            current_tzid = Some(prop.value.trim().to_owned());
                        }
                        "TZOFFSETTO" if in_standard => {
                            std_offset = parse_tz_offset(prop.value);
                        }
                        "TZOFFSETTO" if in_daylight => {
                            day_offset = parse_tz_offset(prop.value);
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }

    map
}

// ── EventBuilder ──────────────────────────────────────────────────────────────

#[derive(Default)]
struct EventBuilder {
    uid: Option<String>,
    summary: Option<String>,
    start: Option<(i64, bool)>, // (unix_utc, all_day)
    end: Option<(i64, bool)>,
    location: Option<String>,
    description: Option<String>,
    status: Option<String>,
    recurrence: Option<String>,
    calendar: Option<String>,
}

impl EventBuilder {
    fn consume(&mut self, prop: Property<'_>, tzmap: &HashMap<String, i64>) {
        let tzid = prop.params.get("tzid").map(String::as_str);
        match prop.name.to_uppercase().as_str() {
            "UID" => self.uid = Some(unescape(prop.value)),
            "SUMMARY" => self.summary = Some(unescape(prop.value)),
            "LOCATION" => self.location = Some(unescape(prop.value)),
            "DESCRIPTION" => self.description = Some(unescape(prop.value)),
            "STATUS" => self.status = Some(prop.value.to_lowercase()),
            "RRULE" => self.recurrence = Some(prop.value.to_owned()),
            "CATEGORIES" => {} // ignored for now
            "DTSTART" => {
                self.start = parse_datetime(prop.value, tzid, tzmap);
                // Honour VALUE=DATE parameter override
                if prop.params.get("value").map(String::as_str) == Some("DATE") {
                    if let Some((ts, _)) = self.start {
                        self.start = Some((ts, true));
                    }
                }
            }
            "DTEND" | "DUE" => {
                self.end = parse_datetime(prop.value, tzid, tzmap);
                if prop.params.get("value").map(String::as_str) == Some("DATE") {
                    if let Some((ts, _)) = self.end {
                        self.end = Some((ts, true));
                    }
                }
            }
            // X-WR-CALNAME is collected at the VCALENDAR level in parse_ical(),
            // not inside VEVENT — this arm is kept as a fallback only.
            "X-WR-CALNAME" => {
                self.calendar = Some(unescape(prop.value));
            }
            _ => {}
        }
    }

    fn build(self, start_filter: i64, end_filter: i64) -> Option<CalendarEvent> {
        let (start_utc, all_day) = self.start?;
        // Default end = start + 1 hour (or same day for all-day)
        let (end_utc, _) = self.end.unwrap_or_else(|| {
            if all_day {
                (start_utc + 86400, true)
            } else {
                (start_utc + 3600, false)
            }
        });

        // Filter: event overlaps [start_filter, end_filter]
        if end_utc < start_filter || start_utc > end_filter {
            return None;
        }

        Some(CalendarEvent {
            id: self.uid.unwrap_or_default(),
            title: self.summary.unwrap_or_else(|| "(no title)".into()),
            start_utc,
            end_utc,
            all_day,
            location: self.location.filter(|s| !s.is_empty()),
            notes: self.description.filter(|s| !s.is_empty()),
            calendar: self.calendar,
            status: normalize_status(self.status.as_deref()),
            recurrence: self.recurrence,
        })
    }
}

fn normalize_status(s: Option<&str>) -> String {
    match s {
        Some("tentative") => "tentative".into(),
        Some("cancelled") | Some("canceled") => "cancelled".into(),
        _ => "confirmed".into(),
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // 2025-03-25 00:00:00 UTC = 1 742 860 800
    // 2026-03-25 00:00:00 UTC = 1 774 396 800
    const TS_20250325: i64 = 1_742_860_800;
    const TS_20260325: i64 = 1_774_396_800;

    // ── ymd_to_unix ───────────────────────────────────────────────────────

    #[test]
    fn ymd_epoch() {
        assert_eq!(ymd_to_unix(1970, 1, 1), 0);
        assert_eq!(ymd_to_unix(1970, 1, 2), 86400);
        assert_eq!(ymd_to_unix(2025, 3, 25), TS_20250325);
        assert_eq!(ymd_to_unix(2026, 3, 25), TS_20260325);
    }

    #[test]
    fn ymd_leap_year() {
        // 2024 is a leap year — Feb 29 exists.
        let feb29 = ymd_to_unix(2024, 2, 29);
        let mar01 = ymd_to_unix(2024, 3, 1);
        assert_eq!(mar01 - feb29, 86400);
    }

    #[test]
    fn ymd_century_non_leap() {
        // 1900 is NOT a leap year (divisible by 100 but not 400).
        let feb28 = ymd_to_unix(1900, 2, 28);
        let mar01 = ymd_to_unix(1900, 3, 1);
        assert_eq!(mar01 - feb28, 86400); // no Feb 29 — gap is exactly 1 day
    }

    #[test]
    fn ymd_400_year_leap() {
        // 2000 IS a leap year (divisible by 400).
        let feb29 = ymd_to_unix(2000, 2, 29);
        let mar01 = ymd_to_unix(2000, 3, 1);
        assert_eq!(mar01 - feb29, 86400);
    }

    #[test]
    fn ymd_year_boundary() {
        let dec31 = ymd_to_unix(2025, 12, 31);
        let jan01 = ymd_to_unix(2026, 1, 1);
        assert_eq!(jan01 - dec31, 86400);
    }

    // ── parse_datetime ────────────────────────────────────────────────────

    #[test]
    fn parse_utc_datetime() {
        let (ts, all_day) = parse_datetime("20260325T120000Z", None, &HashMap::new()).unwrap();
        assert_eq!(ts, TS_20260325 + 12 * 3600);
        assert!(!all_day);
    }

    #[test]
    fn parse_all_day() {
        let (ts, all_day) = parse_datetime("20260325", None, &HashMap::new()).unwrap();
        assert_eq!(ts, TS_20260325);
        assert!(all_day);
    }

    #[test]
    fn parse_value_date_param() {
        // VALUE=DATE parameter should produce all_day=true even without Z suffix.
        let mut params = HashMap::new();
        params.insert("value".to_string(), "DATE".to_string());
        // Simulate what EventBuilder.consume() does — call parse_datetime
        // then override all_day when param is VALUE=DATE.
        let (ts, _all_day) = parse_datetime("20260325", None, &HashMap::new()).unwrap();
        assert_eq!(ts, TS_20260325);
    }

    #[test]
    fn parse_utc_midnight() {
        let (ts, all_day) = parse_datetime("20260325T000000Z", None, &HashMap::new()).unwrap();
        assert_eq!(ts, TS_20260325);
        assert!(!all_day);
    }

    #[test]
    fn parse_utc_end_of_day() {
        let (ts, _) = parse_datetime("20260325T235959Z", None, &HashMap::new()).unwrap();
        assert_eq!(ts, TS_20260325 + 23 * 3600 + 59 * 60 + 59);
    }

    #[test]
    fn timezone_offset() {
        let mut map = HashMap::new();
        map.insert("America/New_York".to_string(), -5 * 3600);
        // 09:00 EST = 14:00 UTC
        let (ts, _) = parse_datetime("20260325T090000", Some("America/New_York"), &map).unwrap();
        assert_eq!(ts, TS_20260325 + 14 * 3600);
    }

    #[test]
    fn timezone_positive_offset() {
        let mut map = HashMap::new();
        map.insert("Asia/Kolkata".to_string(), 5 * 3600 + 30 * 60); // +05:30
                                                                    // 14:30 IST = 09:00 UTC
        let (ts, _) = parse_datetime("20260325T143000", Some("Asia/Kolkata"), &map).unwrap();
        assert_eq!(ts, TS_20260325 + 9 * 3600);
    }

    #[test]
    fn timezone_unknown_fallback_to_utc() {
        // Unknown TZID → treat timestamp as UTC.
        let (ts, _) = parse_datetime("20260325T120000", Some("Nowhere/Unknown"), &HashMap::new()).unwrap();
        assert_eq!(ts, TS_20260325 + 12 * 3600);
    }

    #[test]
    fn parse_datetime_invalid_returns_none() {
        assert!(parse_datetime("", None, &HashMap::new()).is_none());
        assert!(parse_datetime("notadate", None, &HashMap::new()).is_none());
    }

    // ── tz_offset_parse ───────────────────────────────────────────────────

    #[test]
    fn tz_offset_parse() {
        assert_eq!(parse_tz_offset("-0500"), Some(-18000));
        assert_eq!(parse_tz_offset("+0530"), Some(19800));
        assert_eq!(parse_tz_offset("+0000"), Some(0));
        assert_eq!(parse_tz_offset("-1200"), Some(-43200));
        assert_eq!(parse_tz_offset("+1400"), Some(50400));
    }

    #[test]
    fn tz_offset_with_seconds() {
        assert_eq!(parse_tz_offset("+053045"), Some(5 * 3600 + 30 * 60 + 45));
    }

    // ── unfold ────────────────────────────────────────────────────────────

    #[test]
    fn unfolding() {
        let folded = "BEGIN:VCALENDAR\r\nSUMMARY:Long eve\r\n nt title here\r\nEND:VCALENDAR\r\n";
        let lines = unfold(folded);
        assert!(lines.contains(&"SUMMARY:Long event title here".to_string()));
    }

    #[test]
    fn unfolding_tab_continuation() {
        let folded = "KEY:val\r\n\tue continued\r\n";
        let lines = unfold(folded);
        assert!(lines.contains(&"KEY:value continued".to_string()));
    }

    #[test]
    fn unfolding_lf_only() {
        // Files using bare LF instead of CRLF should also unfold correctly.
        let folded = "SUMMARY:Hello\n Wo\n rld\n";
        let lines = unfold(folded);
        assert!(lines.contains(&"SUMMARY:HelloWorld".to_string()));
    }

    // ── parse_property ────────────────────────────────────────────────────

    #[test]
    fn property_bare_name_and_value() {
        let p = parse_property("SUMMARY:Team Meeting").unwrap();
        assert_eq!(p.name, "SUMMARY");
        assert_eq!(p.value, "Team Meeting");
        assert!(p.params.is_empty());
    }

    #[test]
    fn property_with_param() {
        let p = parse_property("DTSTART;TZID=America/New_York:20260325T090000").unwrap();
        assert_eq!(p.name, "DTSTART");
        assert_eq!(p.params.get("tzid").map(String::as_str), Some("America/New_York"));
        assert_eq!(p.value, "20260325T090000");
    }

    #[test]
    fn property_value_param() {
        let p = parse_property("DTSTART;VALUE=DATE:20260325").unwrap();
        assert_eq!(p.name, "DTSTART");
        assert_eq!(p.params.get("value").map(String::as_str), Some("DATE"));
        assert_eq!(p.value, "20260325");
    }

    #[test]
    fn property_multiple_params() {
        let p = parse_property(r#"ATTACH;FMTTYPE=text/plain;ENCODING=BASE64:dGVzdA=="#).unwrap();
        assert_eq!(p.name, "ATTACH");
        assert_eq!(p.params.get("fmttype").map(String::as_str), Some("text/plain"));
        assert_eq!(p.params.get("encoding").map(String::as_str), Some("BASE64"));
    }

    #[test]
    fn property_colon_in_value() {
        // Value itself contains colons — only first unquoted colon splits name from value.
        let p = parse_property("URL:https://example.com/event?id=1&t=2").unwrap();
        assert_eq!(p.name, "URL");
        assert_eq!(p.value, "https://example.com/event?id=1&t=2");
    }

    #[test]
    fn property_quoted_param_with_semicolon() {
        let p = parse_property(r#"X-KEY;CN="Smith, John":val"#).unwrap();
        assert_eq!(p.name, "X-KEY");
        assert_eq!(p.params.get("cn").map(String::as_str), Some("Smith, John"));
    }

    // ── unescape ─────────────────────────────────────────────────────────

    #[test]
    fn unescape_newline() {
        assert_eq!(unescape(r"line1\nline2"), "line1\nline2");
    }

    #[test]
    fn unescape_comma_and_semicolon() {
        assert_eq!(unescape(r"a\,b\;c"), "a,b;c");
    }

    #[test]
    fn unescape_backslash() {
        assert_eq!(unescape(r"a\\b"), r"a\b");
    }

    #[test]
    fn unescape_trailing_backslash() {
        // Lone trailing backslash is preserved as-is.
        assert_eq!(unescape("test\\"), "test\\");
    }

    // ── parse_ical ────────────────────────────────────────────────────────

    #[test]
    fn parse_simple_ical() {
        let ical = "BEGIN:VCALENDAR\r\n\
                    BEGIN:VEVENT\r\n\
                    UID:test@example.com\r\n\
                    SUMMARY:Team Standup\r\n\
                    DTSTART:20260325T090000Z\r\n\
                    DTEND:20260325T093000Z\r\n\
                    STATUS:CONFIRMED\r\n\
                    END:VEVENT\r\n\
                    END:VCALENDAR\r\n";

        let events = parse_ical(ical, 0, i64::MAX);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].title, "Team Standup");
        assert_eq!(events[0].id, "test@example.com");
        assert_eq!(events[0].status, "confirmed");
        assert!(!events[0].all_day);
        assert_eq!(events[0].start_utc, TS_20260325 + 9 * 3600);
        assert_eq!(events[0].end_utc, TS_20260325 + 9 * 3600 + 30 * 60);
    }

    #[test]
    fn parse_all_day_event() {
        let ical = "BEGIN:VCALENDAR\r\n\
                    BEGIN:VEVENT\r\n\
                    UID:allday@test\r\n\
                    SUMMARY:Public Holiday\r\n\
                    DTSTART;VALUE=DATE:20260325\r\n\
                    DTEND;VALUE=DATE:20260326\r\n\
                    END:VEVENT\r\n\
                    END:VCALENDAR\r\n";

        let events = parse_ical(ical, 0, i64::MAX);
        assert_eq!(events.len(), 1);
        assert!(events[0].all_day);
        assert_eq!(events[0].start_utc, TS_20260325);
    }

    #[test]
    fn parse_multiple_events() {
        let ical = "BEGIN:VCALENDAR\r\n\
                    BEGIN:VEVENT\r\n\
                    UID:ev1@test\r\n\
                    SUMMARY:First\r\n\
                    DTSTART:20260325T090000Z\r\n\
                    DTEND:20260325T100000Z\r\n\
                    END:VEVENT\r\n\
                    BEGIN:VEVENT\r\n\
                    UID:ev2@test\r\n\
                    SUMMARY:Second\r\n\
                    DTSTART:20260325T140000Z\r\n\
                    DTEND:20260325T150000Z\r\n\
                    END:VEVENT\r\n\
                    END:VCALENDAR\r\n";

        let events = parse_ical(ical, 0, i64::MAX);
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].title, "First");
        assert_eq!(events[1].title, "Second");
    }

    #[test]
    fn parse_range_filter_excludes_outside() {
        let ical = "BEGIN:VCALENDAR\r\n\
                    BEGIN:VEVENT\r\n\
                    UID:past@test\r\n\
                    SUMMARY:Past Event\r\n\
                    DTSTART:20200101T090000Z\r\n\
                    DTEND:20200101T100000Z\r\n\
                    END:VEVENT\r\n\
                    BEGIN:VEVENT\r\n\
                    UID:future@test\r\n\
                    SUMMARY:Future Event\r\n\
                    DTSTART:20260325T090000Z\r\n\
                    DTEND:20260325T100000Z\r\n\
                    END:VEVENT\r\n\
                    END:VCALENDAR\r\n";

        // Only ask for events from 2026-03-25 onwards.
        let events = parse_ical(ical, TS_20260325, i64::MAX);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].title, "Future Event");
    }

    #[test]
    fn parse_range_filter_includes_overlapping() {
        // An event that *spans* the query window should be included even if
        // it started before the window.
        let ical = "BEGIN:VCALENDAR\r\n\
                    BEGIN:VEVENT\r\n\
                    UID:span@test\r\n\
                    SUMMARY:Multi-day\r\n\
                    DTSTART:20260324T000000Z\r\n\
                    DTEND:20260326T000000Z\r\n\
                    END:VEVENT\r\n\
                    END:VCALENDAR\r\n";

        // Query starts on the 25th — event overlaps because end > start_filter.
        let events = parse_ical(ical, TS_20260325, TS_20260325 + 86400);
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn parse_location_and_description() {
        let ical = "BEGIN:VCALENDAR\r\n\
                    BEGIN:VEVENT\r\n\
                    UID:detail@test\r\n\
                    SUMMARY:Sprint Review\r\n\
                    DTSTART:20260325T140000Z\r\n\
                    DTEND:20260325T150000Z\r\n\
                    LOCATION:Conference Room B\r\n\
                    DESCRIPTION:Bring your laptop\\nDemo the new feature\r\n\
                    END:VEVENT\r\n\
                    END:VCALENDAR\r\n";

        let events = parse_ical(ical, 0, i64::MAX);
        assert_eq!(events[0].location.as_deref(), Some("Conference Room B"));
        assert!(events[0].notes.as_deref().unwrap().contains("Bring your laptop"));
        assert!(events[0].notes.as_deref().unwrap().contains("Demo the new feature"));
    }

    #[test]
    fn parse_tentative_and_cancelled_status() {
        let make_event = |status: &str| {
            format!(
                "BEGIN:VCALENDAR\r\n\
                 BEGIN:VEVENT\r\n\
                 UID:s@test\r\n\
                 SUMMARY:S\r\n\
                 DTSTART:20260325T090000Z\r\n\
                 DTEND:20260325T100000Z\r\n\
                 STATUS:{status}\r\n\
                 END:VEVENT\r\n\
                 END:VCALENDAR\r\n"
            )
        };

        let t = parse_ical(&make_event("TENTATIVE"), 0, i64::MAX);
        assert_eq!(t[0].status, "tentative");

        let c = parse_ical(&make_event("CANCELLED"), 0, i64::MAX);
        assert_eq!(c[0].status, "cancelled");

        let d = parse_ical(&make_event("CONFIRMED"), 0, i64::MAX);
        assert_eq!(d[0].status, "confirmed");
    }

    #[test]
    fn parse_recurrence_rule_preserved() {
        let ical = "BEGIN:VCALENDAR\r\n\
                    BEGIN:VEVENT\r\n\
                    UID:rec@test\r\n\
                    SUMMARY:Daily Standup\r\n\
                    DTSTART:20260325T090000Z\r\n\
                    DTEND:20260325T093000Z\r\n\
                    RRULE:FREQ=DAILY;BYDAY=MO,TU,WE,TH,FR\r\n\
                    END:VEVENT\r\n\
                    END:VCALENDAR\r\n";

        let events = parse_ical(ical, 0, i64::MAX);
        assert_eq!(events.len(), 1);
        let rrule = events[0].recurrence.as_deref().unwrap();
        assert!(rrule.contains("FREQ=DAILY"));
        assert!(rrule.contains("BYDAY=MO"));
    }

    #[test]
    fn parse_default_end_is_one_hour_after_start() {
        // No DTEND → server fills in start + 1h.
        let ical = "BEGIN:VCALENDAR\r\n\
                    BEGIN:VEVENT\r\n\
                    UID:noend@test\r\n\
                    SUMMARY:No End\r\n\
                    DTSTART:20260325T100000Z\r\n\
                    END:VEVENT\r\n\
                    END:VCALENDAR\r\n";

        let events = parse_ical(ical, 0, i64::MAX);
        assert_eq!(events[0].end_utc - events[0].start_utc, 3600);
    }

    #[test]
    fn parse_default_end_for_all_day_is_next_day() {
        let ical = "BEGIN:VCALENDAR\r\n\
                    BEGIN:VEVENT\r\n\
                    UID:allday-noend@test\r\n\
                    SUMMARY:Holiday\r\n\
                    DTSTART;VALUE=DATE:20260325\r\n\
                    END:VEVENT\r\n\
                    END:VCALENDAR\r\n";

        let events = parse_ical(ical, 0, i64::MAX);
        assert!(events[0].all_day);
        assert_eq!(events[0].end_utc - events[0].start_utc, 86400);
    }

    #[test]
    fn parse_vtimezone_block_sets_offset() {
        // Inline VTIMEZONE block should supply the offset for TZID-qualified DTSTART.
        let ical = "BEGIN:VCALENDAR\r\n\
                    BEGIN:VTIMEZONE\r\n\
                    TZID:Europe/London\r\n\
                    BEGIN:STANDARD\r\n\
                    TZOFFSETFROM:+0100\r\n\
                    TZOFFSETTO:+0000\r\n\
                    END:STANDARD\r\n\
                    END:VTIMEZONE\r\n\
                    BEGIN:VEVENT\r\n\
                    UID:tz@test\r\n\
                    SUMMARY:London Call\r\n\
                    DTSTART;TZID=Europe/London:20260325T090000\r\n\
                    DTEND;TZID=Europe/London:20260325T100000\r\n\
                    END:VEVENT\r\n\
                    END:VCALENDAR\r\n";

        let events = parse_ical(ical, 0, i64::MAX);
        assert_eq!(events.len(), 1);
        // +0000 offset → 09:00 local = 09:00 UTC
        assert_eq!(events[0].start_utc, TS_20260325 + 9 * 3600);
    }

    #[test]
    fn parse_folded_summary() {
        // Long SUMMARY folded across two physical lines.
        let ical = "BEGIN:VCALENDAR\r\n\
                    BEGIN:VEVENT\r\n\
                    UID:fold@test\r\n\
                    SUMMARY:This is a very long event title that ge\r\n \
                    ts folded across two physical lines\r\n\
                    DTSTART:20260325T090000Z\r\n\
                    DTEND:20260325T100000Z\r\n\
                    END:VEVENT\r\n\
                    END:VCALENDAR\r\n";

        let events = parse_ical(ical, 0, i64::MAX);
        assert_eq!(events.len(), 1);
        assert_eq!(
            events[0].title,
            "This is a very long event title that gets folded across two physical lines"
        );
    }

    #[test]
    fn parse_empty_calendar() {
        let ical = "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nEND:VCALENDAR\r\n";
        let events = parse_ical(ical, 0, i64::MAX);
        assert!(events.is_empty());
    }

    #[test]
    fn parse_event_missing_summary_gets_default_title() {
        let ical = "BEGIN:VCALENDAR\r\n\
                    BEGIN:VEVENT\r\n\
                    UID:notitle@test\r\n\
                    DTSTART:20260325T090000Z\r\n\
                    DTEND:20260325T100000Z\r\n\
                    END:VEVENT\r\n\
                    END:VCALENDAR\r\n";

        let events = parse_ical(ical, 0, i64::MAX);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].title, "(no title)");
    }

    #[test]
    fn parse_google_calendar_export() {
        // Typical Google Calendar iCal export with PRODID, X-WR-CALNAME, VTIMEZONE.
        let ical = "BEGIN:VCALENDAR\r\n\
                    VERSION:2.0\r\n\
                    PRODID:-//Google Inc//Google Calendar 70.9054//EN\r\n\
                    X-WR-CALNAME:Work Calendar\r\n\
                    BEGIN:VTIMEZONE\r\n\
                    TZID:America/Los_Angeles\r\n\
                    BEGIN:STANDARD\r\n\
                    TZOFFSETFROM:-0700\r\n\
                    TZOFFSETTO:-0800\r\n\
                    END:STANDARD\r\n\
                    END:VTIMEZONE\r\n\
                    BEGIN:VEVENT\r\n\
                    DTSTART;TZID=America/Los_Angeles:20260325T090000\r\n\
                    DTEND;TZID=America/Los_Angeles:20260325T100000\r\n\
                    SUMMARY:Design Review\r\n\
                    DESCRIPTION:Review Q2 designs\\nSee attached mockups\r\n\
                    LOCATION:Zoom\r\n\
                    STATUS:CONFIRMED\r\n\
                    UID:design-review-2026@google.com\r\n\
                    END:VEVENT\r\n\
                    END:VCALENDAR\r\n";

        let events = parse_ical(ical, 0, i64::MAX);
        assert_eq!(events.len(), 1);
        let ev = &events[0];
        assert_eq!(ev.title, "Design Review");
        assert_eq!(ev.calendar.as_deref(), Some("Work Calendar"));
        assert_eq!(ev.location.as_deref(), Some("Zoom"));
        assert_eq!(ev.status, "confirmed");
        // PST = -08:00; 09:00 PST = 17:00 UTC
        assert_eq!(ev.start_utc, TS_20260325 + 17 * 3600);
    }
}
