### Performance

- **Idle re-embed throttle default**: bumped from 10 ms to 200 ms between epochs. The previous value drove the daemon to ~100% CPU on machines without a fast GPU whenever the device was idle. A migration in `load_settings` promotes any existing `idle_reembed_throttle_ms == 10` to 200 and rewrites the file, so users who hit the bug get fixed on next launch.
- **Adaptive scanner cadence**: the auto-started device scanner stays at 5 s while devices are paired or being seen, then backs off to a 30 s tick after 5 minutes of empty scans with no paired devices. Any new discovery (or BLE/USB activity) snaps it back to fast cadence. Previously every transport — USB serial, BLE cache, Cortex, NeuroField, BrainBit, g.tec, ANT Neuro, BrainMaster — was probed forever every 5 s even on installs with no hardware.
- **Active-window poll** slowed from 1 s to 3 s. Still snappy enough for app-switch tracking; ~3× fewer wakeups for the platform window probe (Accessibility on macOS, X11/Wayland calls on Linux).
- **macOS clipboard monitor** now reads everything natively via `objc2-app-kit`: `NSPasteboard.changeCount` for the change gate, `NSPasteboard.types`/`dataForType:` for content classification and size, and `dataForType:NSPasteboardTypePNG` for clipboard image capture. Removes every `osascript` subprocess fork and the Apple Events permission prompt that used to come with it. Steady-state and copy events both run inside the daemon process.

- **Idle re-embed throttle migration logging**: `load_settings` now emits a `tracing::info!` line when it promotes a legacy `idle_reembed_throttle_ms == 10` to 200, so support can confirm the migration ran from the daemon log without diffing the settings file.

### Server

- **`GET /v1/activity`**: new endpoint returns a manifest of every recurring background task the daemon runs, with `name`, `does`, `why`, `interval_secs`, `cost`, `user_toggleable`, plus live state (running flag, idle countdown) and `heartbeat` (`last_tick_unix_ms`, `last_duration_ms`, `tick_count`) read from a central registry on `AppState`. So users who notice CPU usage can see — and challenge — exactly which workers are active rather than guessing.
- **Background task registry + `activity-state` event**: `AppState::record_task_heartbeat(id, duration_ms)` is called once per tick by `device-scanner`, `status-monitor`, `idle-reembed`, `active-window-poll`, `clipboard-monitor`, `tty-embedder`, `reconnect`, and `skills-sync`. Each call updates the registry and (time-throttled per task to one broadcast every 5s, so a 100ms loop wouldn't flood the bus) broadcasts an `activity-state` WebSocket event with the heartbeat payload, so connected clients update without polling. Adding a new background loop without registering its id surfaces as a static row with a zeroed heartbeat — a built-in drift signal. The `idle-reembed` heartbeat additionally fires inside the embed-progress event consumer, so the panel reflects real per-batch wall-clock time rather than the outer 10s polling cadence.

### UI

- **Daemon Background Activity panel** in Settings → Settings tab: lists every recurring daemon task with a one-line description, a `Why:` explanation, interval, cost class, and a live "running"/"idle" badge plus `last ran Ns ago · took X ms · N ticks`. Subscribes to the `activity-state` WebSocket event for live updates and falls back to a 30 s safety-net `/v1/activity` poll. Users can decide which trackers to disable based on what each one is actually for and how active it currently is.

### i18n

- Translated all `daemonActivity.*` keys (title, intro, loading, running, idle, eventDriven, whyPrefix, costLow/Medium/High, never, lastRanSecondsAgo / MinutesAgo / HoursAgo, tickDuration, tickCount) into all 9 locales: `en`, `de`, `es`, `fr`, `he`, `ja`, `ko`, `uk`, `zh`. Strings are now idiomatic (e.g. ES "carga baja" instead of "coste bajo", JA "実行回数: {n}" instead of "{n} 回実行", FR "il y a {n} s" instead of "{n} ms écoulées") and avoid singular/plural mismatches by using register-neutral phrasings ("Cycles : {n}" rather than "{n} exécutions").
