### Features

- **Updater: no auto-restart, session-safe**: Updates are now downloaded automatically in the background, but the app no longer auto-restarts after downloading. The user must explicitly click "Restart Now" to apply. Restarting is **blocked** while an EEG session is being recorded — a warning is shown and the restart button is disabled until the session ends. Only the user can bypass this protection. When quitting the app (via menu, tray, or Cmd+Q), if a downloaded update is staged, the app relaunches to apply it instead of just exiting.
