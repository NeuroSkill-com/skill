### Features

- **Paired devices persisted by the daemon** (`paired_devices.json`):
  `pair_device` and `forget_device` write a dedicated
  `~/.skill/paired_devices.json` on every change.  The file is written
  atomically (temp-file + rename) so a crash mid-write never corrupts it.
  The daemon reloads it at startup to populate `status.paired_devices`
  before the first scanner tick, meaning paired devices survive restarts
  without Tauri needing to re-sync them.  `settings.json` is also kept in
  sync via a background task for backward compatibility with older builds and
  the Tauri side.  `skill-constants::PAIRED_DEVICES_FILE` is the shared
  constant used by both daemon and Tauri.

- **Tauri reads `paired_devices.json` directly at startup**:
  `load_and_apply_settings` now prefers `paired_devices.json`
  (daemon-authoritative) over `settings.json` (which may lag by the async
  write).  Paired devices appear correctly in the UI on the very first render
  frame, before the daemon status poll completes.

- **Muse firmware version surfaced at runtime**: the session runner now
  handles `DeviceEvent::Meta` (previously silently discarded in the `_ => {}`
  branch).  Muse Control JSON responses contain a `"fw"` field (e.g.
  `"3.4.5"`); when one arrives, `status.firmware_version` is updated and a
  `StatusUpdate` WebSocket event is broadcast to connected clients.

### Bugfixes

- **Paired devices showed as unpaired after daemon restart**: the scanner
  tick's merge loop copied `is_paired` from `old` (previous in-memory device
  list), which is empty on the first tick after a restart.  Every
  re-discovered device was therefore marked `is_paired = false`.  Fixed: the
  merge builds a `paired_ids` set from `status.paired_devices` (authoritative,
  restored from disk) and checks it first; `old` is used only as a fallback
  for the `is_preferred` flag, which is not persisted.

- **`firmware_version` not synced from daemon to Tauri**: `apply_daemon_status()`
  copied `hardware_version` from `StatusResponse` but silently skipped
  `firmware_version`.  After the new Meta handler sets it, Tauri now receives
  it via the status poll.

- **Dead `hardware_version === "p50"` Athena detection**: both
  `devices-logic.ts` and `+page.svelte` checked `hw === "p50"` /
  `hardware_version === "p50"` to identify the Muse S Athena firmware.
  `p50` is a BLE preset command string, not a hardware version identifier,
  and the Muse adapter never sets `hardware_version`.  The condition was
  permanently `false` dead code.  Removed; the correct name-based detection
  (`"muses"` substring in the advertising name) is now the sole check.
  The corresponding Vitest test was updated to document the correct
  behaviour (`"Muse-S"` + `hw="p50"` → Classic gen1 image; `"MuseS-F921"`
  with no hw arg → Athena image).

- **Non-atomic JSON writes could leave corrupt files**: `persist_paired_devices()`
  previously used `std::fs::write()` directly for both `paired_devices.json`
  and the `settings.json` sync.  A crash or OS signal between truncation and
  completion would leave a zero-byte or partially-written file.  Both writes
  now use `write_json_atomic()` (write to `.tmp` sibling, then `rename()`),
  which is atomic on POSIX systems.
