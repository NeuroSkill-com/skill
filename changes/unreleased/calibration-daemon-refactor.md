### Refactor

- Move calibration timing loop from frontend to daemon. The daemon now drives all calibration phases (action countdowns, breaks, label submission) via a session runner with wall-clock-aligned timing. The frontend subscribes to WebSocket events for UI updates.
- Route all calibration commands through daemon HTTP endpoints. Profile CRUD, active profile selection, and calibration completion recording are now daemon-authoritative — the Tauri app is a thin proxy.
- Remove stale calibration state from Tauri `AppState`. Calibration profiles and active profile ID are no longer cached locally, preventing the save overlay from clobbering daemon-written timestamps.
- Delete `calibration_service.rs` — inline daemon HTTP calls directly in `window_cmds.rs`.
- Migrate onboarding calibration wizard to use daemon-driven sessions instead of a local timing loop.
- Remove unused `emit_calibration_event` Tauri command.

### Features

- Add daemon calibration session HTTP routes: `POST /v1/calibration/session/start`, `POST /v1/calibration/session/cancel`, `GET /v1/calibration/session/status`.
- Add `POST /v1/calibration/record-completed` and `GET /v1/calibration/active-profile` daemon routes.
- Daemon broadcasts `calibration-tts` events with spoken text before each phase, giving the frontend a 4-second window to play TTS audio before the countdown begins.

### Bugfixes

- Fix calibration data not being recorded because `record_calibration_completed` wrote to stale local state instead of the daemon.
- Fix `list_calibration_profiles` returning default profiles instead of daemon-authoritative data.

### Build

- Fix macOS CI smoke test checking wrong path for daemon binary (`Contents/MacOS/skill-daemon` → `Contents/MacOS/skill-daemon.app/Contents/MacOS/skill-daemon`).
- Fix Linux CI Vulkan SDK cache restoring system library paths with permission errors. Vulkan SDK is now installed via `cache-apt-pkgs-action` alongside other system dependencies.
- Fix Linux CI daemon link failure: add `cargo:rustc-link-lib=vulkan` and openblas search paths in daemon `build.rs` for prebuilt llama archives.
- Fix Linux CI Discord notification firing on manual dispatch builds.
- Remove duplicate unguarded portable tarball upload step from Linux release workflow.
- Exclude E2E test files from `vitest related` in pre-push hook.
- Add all calibration commands to `check-daemon-invokes.js` validation script.
