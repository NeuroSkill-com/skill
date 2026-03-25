### Features

- **Calendar event fetching**: New `skill-calendar` crate adds cross-platform calendar support.
  - **macOS**: Reads all calendars via Apple EventKit (`EKEventStore`) using Objective-C FFI — covers iCloud, Google, Exchange, and local calendars synced to Calendar.app. Handles the macOS 14+ `requestFullAccessToEventsWithCompletion:` API with fallback to the legacy API on older systems.
  - **Linux**: Scans XDG locations for `.ics` files from GNOME Calendar, Evolution, KOrganizer, Thunderbird, and `~/Calendars/`.
  - **Windows**: Scans Outlook / Windows Calendar paths under `%APPDATA%`, `%LOCALAPPDATA%`, and UWP package directories for `.ics` files.
  - Shared RFC 5545 iCal parser: handles line folding, `VTIMEZONE` offset extraction, `VALUE=DATE` all-day events, UTC (`Z`) timestamps, iCal escape sequences, and recurrence rule passthrough.
  - Three new WebSocket commands: `calendar_events` (fetch by range), `calendar_status` (auth state + platform), `calendar_request_permission` (macOS permission dialog).
  - `NSCalendarsUsageDescription` added to `Info.plist`; `com.apple.security.personal-information.calendars` entitlement added to `entitlements.plist`.
