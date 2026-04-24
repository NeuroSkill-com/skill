### Features

- **Meeting detection**: automatically detect Zoom, Teams, Slack, Google Meet, FaceTime, Discord, and Webex meetings from window titles. Track meeting start/end times with platform attribution in new `meeting_events` SQLite table.
- **Browser tab extraction**: extract page titles from Chrome, Safari, Firefox, Edge, Brave, Arc, Opera, Vivaldi, and Chromium browser window titles. Stored in new `browser_title` column on `active_windows`.
- **Clipboard monitoring**: opt-in macOS clipboard change tracking (metadata only — content never stored). Records source app, content type, and size in new `clipboard_events` table. Includes Automation permission check and settings UI.
- **Multi-monitor awareness**: track windows visible on secondary monitors across macOS (AppleScript), Linux (wmctrl), and Windows (EnumWindows). Stored in new `secondary_windows` table with monitor_id attribution.
- **Undo frequency tracking**: detect undo/redo activity from file edit diffs using reversal heuristic. `undo_estimate` tracked per 5-second edit chunk and per file interaction.
- **Activity dashboard**: new Activity tab in Settings showing today's summary, productivity score, hourly heatmap, top files/projects, language breakdown, focus sessions, meetings, and stale file alerts.
- **Focus session replay**: click any focus session to expand a detailed view showing every file worked on, timestamps, edit counts, EEG focus overlay bar, and meeting interruptions.
- **EEG overlay on file timeline**: focus score plotted as a color-coded bar alongside file activity in session replay, showing correlation between brain state and coding activity.
- **Weekly report with CSV export**: load a 7-day activity digest with daily breakdown chart, summary stats, and one-click CSV export for project time reporting.
- **Weekly digest notification**: automated Monday 9am OS notification with weekly summary (hours coded, edits, files, meetings, peak hour). Cross-platform: macOS (osascript), Linux (notify-send), Windows (PowerShell toast).
- **Productivity scoring**: composite 0–100 score from edit velocity, deep work time, context stability, and EEG focus. Available via `/activity/productivity-score` endpoint.
- **Weekly digest API**: aggregated 7-day activity summary with peak day/hour, top projects, languages, and meeting counts. Available via `/activity/weekly-digest`.
- **Stale file detection**: identify files edited but untouched for 7+ days. Available via `/activity/stale-files`.
- **File activity in history**: expanded EEG sessions now show which files were worked on during the session, with focus indicators, edit counts, and meeting interruptions.
- **File activity in search**: interactive search graph includes `file_activity` nodes linked to EEG epochs and text labels, showing which files were being edited during matching brain states.

### i18n

- Added activity dashboard, clipboard tracking, file history, session replay, weekly report, and EEG overlay translations to all 9 locales (en, de, es, fr, he, ja, ko, uk, zh).

### Docs

- **VS Code extension plan**: added `docs/vscode-extension.md` design document for future editor integration.
