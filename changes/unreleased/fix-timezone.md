### Bugfixes

- **Timezone-wrong period classification**: `daily_brain_report`, `hourly_edit_heatmap`, and `optimal_hours` used `seen_at % 86400` (UTC hour) instead of local hour. Daily report now uses `seen_at - day_start`; heatmap and optimal-hours accept a timezone offset computed server-side.
- **UTC midnight in all clients**: search page, CLI (`cmdActivity`, `cmdBrain`), VS Code extension, and Swift widget all computed `day_start` as UTC midnight instead of local midnight. Fixed to use `Date.setHours(0,0,0,0)` (JS/TS) and `Calendar.startOfDay` (Swift).
- **Widget empty daily-report body**: `DaemonClient.fetchDailyReport()` sent an empty POST body; the endpoint requires `dayStart`. Now sends local midnight.
- **Widget UTC sleep/calendar endpoints**: `fetchSleep()` and `fetchCalendarEvents()` used `% 86400` UTC approximations. Now use `Calendar.current` for correct local time boundaries.
