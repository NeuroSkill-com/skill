### Features

- **Daemon logs in dev terminal**: `npm run tauri dev` now streams daemon
  `tracing` output into the same terminal as Tauri logs.  A custom
  `TeeWriter` tees every log event to both stderr and a 512-line ring buffer
  in `AppState`; `GET /v1/log/recent?since=<seq>` exposes the buffer; the
  Tauri process polls it every second and `eprintln!`s new lines, which the
  existing stderr tee also saves to the session log file.

- **Event-driven BLE device discovery** replaces the old polling approach
  that created a new `BtManager` (and thus a new `CBCentralManager`) on
  every 5-second scanner tick.  A single `run_ble_listener_task` now holds
  the adapter alive for the lifetime of the scanner, subscribes to
  `CentralEvent::DeviceDiscovered` / `DeviceUpdated`, and updates a shared
  `ble_device_cache` as advertisements arrive.  This ensures `local_name`
  is populated (CoreBluetooth fills it asynchronously; it was often `None`
  in a short poll window) and means the Muse S shows up by name instead of
  as an anonymous UUID.

- **EEG-only BLE filter**: the BLE listener now only surfaces devices whose
  advertising name matches a known EEG/neurofeedback pattern (`muse*`,
  `ganglion*`, `mw75`, `brainbit`, `unicorn`, `hermes`, `mendi`, `idun`,
  etc.).  Previously every BLE peripheral in range — phones, speakers,
  watches, UUIDs with no name — appeared in the Discovered Devices list.

- **Fast BLE connect path** for all five BLE device families (Muse, MW75,
  Hermes, Mendi, IDUN Guardian): replaced the fixed-sleep `scan_all()` call
  (3–5 s) with `connect()`, which polls `adapter.peripherals()` every 250 ms
  and exits as soon as the target device appears.  For a paired device that
  is already advertising this drops the scan phase from ~5 s to ~250 ms.
  The paired device's name is looked up from `status.paired_devices` and
  passed as `name_prefix` so `connect()` matches the exact headset rather
  than the first device of that family.  A slow `scan_all()` fallback is
  kept for Muse only when no name is known (first-time unpaired connect).

### Bugfixes

- **Muse (and all BLE devices) stuck at "Scanning for device…" forever**:
  when a connection attempt launched its own `CBCentralManager` scan while
  `run_ble_listener_task` was already holding a concurrent
  `CBCentralManager` scan, macOS suppressed the
  `centralManager(_:didConnect:)` delegate callback, causing
  `peripheral.connect()` to hang regardless of the timeout.  Fixed with a
  `ble_scan_paused` `AtomicBool` in `AppState`: the listener task stops its
  scan and parks whenever the flag is set; `connect_device()` sets the flag
  before delegating to the actual connect function and clears it
  unconditionally on return (success or failure).

- **`needs_ble_pause` covers all BLE-scanning device families**: Muse,
  MW75 Neuro, Hermes V1, Mendi fNIRS, IDUN Guardian (`idun`/`guardian`/
  `ige`), and OpenBCI Ganglion were all affected by the two-concurrent-scan
  bug.  The pause logic is now centralised in `connect_device()` rather than
  duplicated in each connect function.

- **Stale `ble_scan_paused` flag after scanner restart**: if a connection
  was in progress when the scanner was stopped, the flag could be left `true`
  and the freshly spawned listener task would stall indefinitely.
  `control_scanner_start()` now clears the flag unconditionally.

- **BLE discovery stops after pressing Cancel mid-connect**:
  `control_cancel_retry()` cancelled the session handle but did not clear
  `ble_scan_paused`.  The BLE listener remained parked with its scan stopped,
  so no BLE devices appeared in the Discovered list until the user either
  restarted the scanner or started another connection attempt.  Fixed:
  `control_cancel_retry()` now clears the flag immediately.

- **BLE event-loop timeout reduced from 2 s to 300 ms** so the
  `ble_scan_paused` flag is detected quickly; the corresponding settling
  pause was reduced from 600 ms to 400 ms.

- **BLE cache TTL extended from 60 s to 120 s** to cover the worst-case
  connection window (400 ms pause + 5 s scan + 10 s connect + 15 s service
  discovery) without a cached device expiring mid-attempt.

- **Daemon log `TeeWriter` never committed to ring buffer**:
  `tracing-subscriber` calls `write_all()` once per event but never calls
  `flush()`.  The original implementation only committed lines in `flush()`,
  so the buffer was always empty.  Fixed: `write()` now detects the trailing
  `\n` that marks a complete tracing event and commits immediately; `flush()`
  is kept as a fallback for partial lines.

- **Muse target UUID ignored on connect**: `connect_muse()` previously
  scanned for the first Muse in range regardless of which device was paired.
  In multi-headset environments the wrong device could be selected.  The BLE
  UUID from `ble:<uuid>` targets is now used to filter `scan_all()` results
  in the slow-path fallback.
