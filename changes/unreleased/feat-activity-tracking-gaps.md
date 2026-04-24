### Features

- **Meeting detection**: automatically detect Zoom, Teams, Slack, Google Meet, FaceTime, Discord, and Webex meetings from window titles. Track meeting start/end times with platform attribution in new `meeting_events` SQLite table.
- **Browser tab extraction**: extract page titles from Chrome, Safari, Firefox, Edge, Brave, Arc, Opera, Vivaldi, and Chromium browser window titles. Stored in new `browser_title` column on `active_windows`.
- **Clipboard monitoring**: opt-in macOS clipboard change tracking (metadata only — content never stored). Records source app, content type, and size in new `clipboard_events` table. Includes Automation permission check and settings UI.
- **Activity dashboard**: new Activity tab in Settings showing today's summary, productivity score, hourly heatmap, top files/projects, language breakdown, focus sessions, meetings, and stale file alerts.
- **Productivity scoring**: composite 0–100 score from edit velocity, deep work time, context stability, and EEG focus. Available via `/activity/productivity-score` endpoint.
- **Weekly digest**: aggregated 7-day activity summary with peak day/hour, top projects, languages, and meeting counts. Available via `/activity/weekly-digest`.
- **Stale file detection**: identify files edited but untouched for 7+ days. Available via `/activity/stale-files`.
- **File activity in history**: expanded EEG sessions now show which files were worked on during the session, with focus indicators, edit counts, and meeting interruptions.
- **File activity in search**: interactive search graph includes `file_activity` nodes linked to EEG epochs and text labels, showing which files were being edited during matching brain states.

### i18n

- Added activity dashboard, clipboard tracking, and file history translations to all 9 locales (en, de, es, fr, he, ja, ko, uk, zh).
