### Features

- **Sleep schedule settings**: Added a new "Sleep" section in Settings with configurable bedtime and wake-up time. Includes five presets (Default 23:00–07:00, Early Bird 21:30–05:30, Night Owl 01:00–09:00, Short Sleeper 00:00–06:00, Long Sleeper 22:00–08:00), a 24-hour clock visualization, and duration summary. Sleep window is persisted and can be used for session classification and sleep staging analysis.

### CLI

- **`sleep-schedule` command**: New CLI command to view and update the sleep schedule. Supports `sleep-schedule` (show current) and `sleep-schedule set --bedtime HH:MM --wake HH:MM --preset <id>` (update). Available presets: default, early_bird, night_owl, short_sleeper, long_sleeper.

### Server

- **`sleep_schedule` / `sleep_schedule_set` WS commands**: New WebSocket API commands for reading and writing the sleep schedule configuration. Partial updates supported — only fields present in the request are changed.

### Docs

- **SKILL.md**: Added `sleep-schedule` command reference with examples, HTTP equivalents, JSON response shapes, and preset table.
