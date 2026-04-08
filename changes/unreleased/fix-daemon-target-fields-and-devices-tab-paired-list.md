### Bugfixes

- **Muse connected but no EEG data streamed**: daemon Muse connect paths now
  explicitly start the stream after BLE/GATT connect by calling
  `MuseHandle::start(true, false)` and then best-effort
  `request_device_info()`.  Previously the session could report
  "connected" while receiving zero samples (flat waveforms, CSV header only).

- **Retry connect could silently no-op** when `status.target_name` was empty.
  Daemon retry target resolution now falls back through:
  `target_id` → legacy `target_name` → preferred discovered device → first
  paired device, and writes clearer session log messages.

- **Connecting target naming is now daemon-authoritative**:
  `StatusResponse` gained canonical fields:
  - `target_id` (stable device id like `ble:...`)
  - `target_display_name` (human-readable paired name)

  Session control and routing paths (`start/switch/retry/spawn`) now set these
  fields from paired metadata, instead of forcing UI heuristics.

- **Potential daemon deadlock in connect status updates fixed**:
  target-field resolution previously re-locked `status` while already holding
  `status.lock()`.  Resolution is now performed outside lock scopes.

- **Settings → Devices paired list could appear empty** if paired devices were
  not currently in the discovered list.  Devices tab now reads authoritative
  `status.paired_devices`, merges them into local rows, and keeps them visible
  even when not actively advertising.

- **Manual-connect hints were mixed into live discovered hardware** in
  Settings → Devices.  The built-in `neurosky` and
  `brainvision:127.0.0.1:51244` rows are now rendered under a dedicated
  **Manual connection hints** subsection instead of the normal discovered
  hardware list.

- **i18n cleanup**: the new manual-hints subsection uses translation keys
  (`devices.manualHints`, `devices.manualHintsHint`) instead of hardcoded
  English text.

- **Tray/dashboard connect labels now prefer canonical fields**:
  runtime rendering uses `target_display_name` + `target_id` instead of legacy
  `target_name` fallbacks.
