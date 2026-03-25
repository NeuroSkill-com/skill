# skill-calendar

Cross-platform calendar event fetching for NeuroSkill.

| Platform | Backend |
|----------|---------|
| macOS    | Apple EventKit (`EKEventStore`) via Objective-C FFI — reads all calendars the user has added to Calendar.app (iCloud, Google, Exchange, local) |
| Linux    | Parses `.ics` files from XDG locations: GNOME Calendar, Evolution, KOrganizer, Thunderbird, `~/Calendars/` |
| Windows  | Parses `.ics` files from Outlook / Windows Calendar app paths under `%APPDATA%`, `%LOCALAPPDATA%`, and UWP package directories |

## API

```rust
use skill_calendar::{auth_status, fetch_events, request_access, AuthStatus};

// 1. Check / request permission (macOS only; always granted elsewhere)
if auth_status() == AuthStatus::NotDetermined {
    request_access(); // shows system permission dialog
}

// 2. Fetch events
let now = /* unix timestamp */;
let events = fetch_events(now, now + 7 * 86400)?;
for ev in &events {
    println!("[{}] {} @ {}", ev.calendar.as_deref().unwrap_or("?"), ev.title, ev.start_utc);
}
```

## CalendarEvent fields

| Field        | Type            | Description |
|--------------|-----------------|-------------|
| `id`         | `String`        | Unique UID |
| `title`      | `String`        | Summary / event name |
| `start_utc`  | `i64`           | UTC unix seconds |
| `end_utc`    | `i64`           | UTC unix seconds |
| `all_day`    | `bool`          | True for date-only events |
| `location`   | `Option<String>`| Location string |
| `notes`      | `Option<String>`| Description / notes |
| `calendar`   | `Option<String>`| Calendar / account name |
| `status`     | `String`        | `"confirmed"`, `"tentative"`, or `"cancelled"` |
| `recurrence` | `Option<String>`| Raw RRULE string (not expanded) |

## WebSocket commands

| Command | Parameters | Description |
|---------|-----------|-------------|
| `calendar_events` | `start_utc`, `end_utc` | Fetch events in range |
| `calendar_status` | — | Return auth status + platform |
| `calendar_request_permission` | — | macOS only: show permission dialog |

## macOS entitlement

The app must have `com.apple.security.personal-information.calendars` in
`entitlements.plist` and `NSCalendarsUsageDescription` in `Info.plist`.
Both are already added by this PR.
