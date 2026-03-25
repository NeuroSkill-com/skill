### Features

- **Calendar event fetching**: New `skill-calendar` crate adds cross-platform OS calendar support.
  - **macOS**: Reads all calendars via Apple EventKit (`EKEventStore`) using Objective-C FFI — covers iCloud, Google, Exchange, and local calendars synced to Calendar.app. Handles the macOS 14+ `requestFullAccessToEventsWithCompletion:` API (requires `NSCalendarsFullAccessUsageDescription`) with fallback to the legacy API on macOS 10.15–13.
  - **Linux**: Scans XDG locations for `.ics` files: GNOME Calendar, Evolution, KOrganizer, Thunderbird Lightning (targeted `calendar-data/` subdir only — not the full mail profile), and `~/Calendars/`.
  - **Windows**: Scans Outlook / Windows Calendar paths under `%APPDATA%`, `%LOCALAPPDATA%`, and UWP package directories for `.ics` files.
  - Shared RFC 5545 iCal parser: line folding (CRLF/LF/tab), `VTIMEZONE` UTC-offset extraction, `VALUE=DATE` all-day events, UTC (`Z`) timestamps, iCal escape sequences (`\n`, `\,`, `\;`, `\\`), `X-WR-CALNAME` (Google Calendar name at VCALENDAR level), recurrence rule passthrough. 46 unit tests.
  - **WS commands**: `calendar_events` (fetch by range), `calendar_status` (auth state + platform), `calendar_request_permission` (macOS system dialog). Both potentially-blocking commands run via `spawn_blocking`.
  - **HTTP REST**: `POST /v1/calendar/events`, `GET /v1/calendar/status`, `POST /v1/calendar/permission` (+ unversioned aliases).
  - **CLI**: `calendar [--start --end]`, `calendar status`, `calendar permission`.
  - **LLM tools**: `calendar_events`, `calendar_status`, `calendar_request_permission` in the `skill` tool enum and `is_skill_api_command` registry. `"calendar"` alias in `resolve_skill_alias`.
  - **Skill markdown**: `skills/skills/neuroskill-calendar/SKILL.md` with LLM tool examples, timestamp arithmetic, common query patterns, and access troubleshooting.
  - **macOS entitlements**: `com.apple.security.personal-information.calendars` added to `entitlements.plist`; `NSCalendarsUsageDescription` and `NSCalendarsFullAccessUsageDescription` added to `Info.plist`.
  - `calendar_events`, `calendar_status`, `calendar_request_permission` added to `skill-router::COMMANDS` public registry.

### Bugfixes

- **Calendar Linux dedup (critical)**: `linux.rs` two-pass deduplication silently dropped every event with a non-empty UID — the first loop pre-inserted all UIDs into `seen`, causing the second loop's `!seen.contains` check to always be false. Both `linux.rs` and `windows.rs` now use single-pass atomic check-and-insert. Anonymous-event keys use NUL separator (`"\0"`) to prevent hash collisions.
- **Calendar `X-WR-CALNAME` scope**: Google Calendar exports place the calendar name at the `VCALENDAR` level, not inside `VEVENT`. The property was only scanned inside `VEVENT` and therefore never populated. Fixed with a top-level pass in `parse_ical()`.
- **Calendar async blocking**: `calendar_events` and `calendar_request_permission` called blocking ObjC EventKit code directly on the tokio async executor thread (up to 30 s for the macOS permission dialog). Both are now `async` and dispatch to `spawn_blocking`.
- **Calendar macOS 14 privacy key**: `NSCalendarsFullAccessUsageDescription` was missing; without it `requestFullAccessToEventsWithCompletion:` silently fails on Sonoma/macOS 14+.
- **Calendar Thunderbird scan**: `~/.thunderbird` root was searched to depth 6, walking the entire mail profile (potentially GB of data). Now only `~/.thunderbird/<profile>/calendar-data/` is scanned.
- **Calendar EventKit error detection**: `macos.rs` used `json_str.contains("\"error\"")` to detect access-denied responses — a false positive for any event whose title or description contains the word `"error"`. Replaced with proper JSON type-based dispatch: `Array` → success, `Object` with `"error"` key → propagate error string.
- **Calendar EventKit write-only access**: When the user chose "Add Events Only" (`EKAuthorizationStatusWriteOnly`, status 4), the fetch proceeded and returned an empty array instead of an error. Now returns `{"error":"calendar_write_only_access"}` so clients can prompt the user to grant full read access.
- **LLM e2e test non-exhaustive match**: `llm_e2e.rs` match on `ToolEvent` was non-exhaustive after `RoundComplete { .. }` was added to the enum; added the missing arm.
- **`skill-calendar` unused dependency**: `anyhow` was declared in `Cargo.toml` but never used in the crate; removed.
