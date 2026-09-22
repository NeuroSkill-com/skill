### Refactor

- **BLE scanner moved to webbluetooth**: the daemon's advertisement scanner now runs on the process-wide `Bluetooth::shared()` session instead of its own btleplug `CBCentralManager`, so it shares one radio session with muse-rs rather than competing with it. The session coexists with an in-flight connect by design, which removes the need to tear the scanner down and rebuild it around every BLE connect attempt.

### Dependencies

- **muse-rs 0.1 → 0.2**: switches the Muse BLE backend from btleplug to webbluetooth. The `MuseClient` / `MuseClientConfig` / `MuseDevice` / `MuseHandle` / `MuseEvent` API is unchanged, so the adapter and connect paths needed no edits.
- **webbluetooth 0.0.1** added as a direct dependency of `skill-daemon`, replacing its direct `btleplug` dependency. Its backends have no native dependencies — no `bluer`, no `dbus`, no `windows` crates — which drops a runtime `libdbus` requirement from the Linux packages.

### Bugfixes

- **Paired devices survive the BLE backend change**: webbluetooth reports Apple device ids as uppercase `NSUUID` strings where btleplug reported lowercase UUIDs, which would have made every entry in `paired_devices.json` stop matching after upgrade — the headset would reappear as unpaired and auto-reconnect would never fire. BLE ids are now canonicalised to one spelling (`skill_daemon_common::ble_id`) at every write, compared case-insensitively at every lookup, and migrated in place on first start.
- **"Forget device" no longer silently fails**: the paired-list removal compared ids with exact string equality, so a case difference left the device paired.
- **Duplicate scanner entries**: a paired device whose stored id differed in case from the scanner's was pushed into the device list a second time, listing the same headset twice.
- **Daemon can request Bluetooth under launchd**: `skill-daemon.app`'s generated `Info.plist` was missing `NSBluetoothAlwaysUsageDescription`. Spawned by the Tauri app the outer bundle's key covered it, but started by launchd (`RunAtLoad`) the daemon is its own responsible process and was denied Bluetooth with no prompt — presenting as a scan that silently found nothing.

### Docs

- **`docs/webbluetooth-migration.md`**: what moved to webbluetooth, what is still on btleplug, and what has to happen before the `[patch.crates-io] btleplug` override can be dropped.
