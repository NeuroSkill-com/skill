### Bugfixes

- **Calibration timer drift**: Replaced sequential `sleep(1000)` countdown with wall-clock-based timing (`Date.now()`) to prevent cumulative drift over long calibration phases.
- **Calibration TTS desynchronization**: The break-phase "Next: …" announcement was fire-and-forget, causing it to queue behind the next action's TTS cue and delay the countdown start. Both break announcements now await completion before the countdown begins, ensuring audio and visual phases stay in sync.
