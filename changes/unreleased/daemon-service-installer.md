### Features

- Auto-install daemon as persistent OS service on first app launch. The Tauri app calls the daemon's `/service/install` endpoint after startup, registering a LaunchAgent (macOS), systemd user unit (Linux), or Windows service. The daemon now persists across app restarts and reboots.
- Add `--uninstall` CLI flag to skill-daemon for clean service removal.

### Bugfixes

- Fix prebuilt llama archives missing `libmtmd` (multimodal library). The collect step in the prebuilt CI workflow now includes `libmtmd*` alongside `libllama*`/`libggml*`/`libcommon*`.
- Bump llama-cpp-4 from 0.2.45 to 0.2.46.
- Fix double daemon spawn on service install. The installer now checks `/service/status` before calling `/service/install`, and `install_launchagent` skips if the plist already exists with a matching binary path.
- Fix LaunchAgent label mismatch. Update hooks (`pre-update.cjs`, `post-update.cjs`) and uninstall script now use `com.skill.daemon` instead of stale `com.neuroskill.skill-daemon`.
- Remove stale `com.neuroskill.skill-daemon.plist` from app bundle Resources.
- Fix daemon logs written to `/tmp/` (wiped on reboot). LaunchAgent now logs to `~/Library/Logs/NeuroSkill/`.
- Add rollback binary as last-resort candidate in daemon path resolver.

### Build

- Pin prebuilt llama download to specific tag (`0.2.46`) in `ci.mjs` instead of floating `latest`.
- Add explicit error and exit on daemon binary missing after prebuilt retry in release workflow.
- Add smoke-test step in release workflow: `codesign --verify`, architecture check with `file`/`lipo`.
- Add `scripts/test-daemon-e2e.sh` — 19 end-to-end tests covering fresh install, update hooks, uninstall, degraded states, edge cases, and connection reuse.

### Refactor

- Make `ensure_daemon_running` non-blocking. Window now appears immediately while daemon connection is established on a background thread.
- Share a single `ureq::Agent` via `OnceLock` instead of creating one per HTTP call.
