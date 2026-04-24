### Features

- **Meeting detection**: automatically detect Zoom, Teams, Slack, Google Meet, FaceTime, Discord, and Webex meetings from window titles. Track start/end times in `meeting_events` table.
- **Browser tab extraction**: extract page titles from Chrome, Safari, Firefox, Edge, Brave, Arc, Opera, Vivaldi, Chromium. Stored in `browser_title` column on `active_windows`.
- **Clipboard monitoring**: opt-in macOS clipboard change tracking (metadata only — content never stored). Records source app, content type, and size. Includes Automation permission check and settings UI.
- **Multi-monitor awareness**: track windows on secondary monitors across macOS (AppleScript), Linux (wmctrl), Windows (EnumWindows). Dynamic primary screen resolution detection.
- **Undo frequency tracking**: detect undo/redo from file edit diffs via reversal heuristic. `undo_estimate` per 5-second chunk, `undo_count` per file interaction.

### Bugfixes

- **Schema migration**: ALTER TABLE for existing databases missing columns added across releases. Runs before DDL to handle all legacy schemas.
- **Retention pruning**: meetings, clipboard, and secondary_windows pruned alongside file interactions during hourly maintenance.
