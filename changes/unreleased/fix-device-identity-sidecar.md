### Features

- **PPG data now written to session CSV/Parquet**: `DeviceEvent::Ppg` frames
  are now passed to `pipe.writer.push_ppg()` in the session runner.
  Previously PPG was only broadcast over WebSocket; it was never recorded.
  Muse PPG (optical heart-rate channels: Ambient, Infrared, Red) now
  appears in session files alongside EEG, IMU, and band metrics.  PPG
  heart-rate / HRV metrics (`PpgAnalyzer`) are now computed by the daemon
  and written alongside raw PPG samples in each epoch.

- **fNIRS recording pipeline**: full end-to-end fNIRS data persistence for
  devices like Mendi that provide optical brain-imaging data instead of EEG.

  - `DeviceEvent::Fnirs(FnirsFrame)` — new event variant carrying raw
    multi-channel photodetector ADC values + device timestamp.
  - `DeviceCaps::FNIRS` flag added to the capability bitflags.
  - `MendiAdapter` now emits `DeviceEvent::Fnirs` (9 channels: IR/Red/Ambient
    × Left/Right/Pulse) instead of packing optical data into a raw-JSON
    `DeviceEvent::Meta`.  Temperature-only diagnostics remain as `Meta`.
  - `SessionCsvWriter::push_fnirs()` — lazily creates `exg_*_fnirs.csv` with
    `timestamp_s` + per-channel columns using the device's
    `fnirs_channel_names`.  Auto-flushes every 64 rows.
  - `SessionParquetWriter::push_fnirs()` — lazily creates `exg_*_fnirs.parquet`
    with a Snappy-compressed schema built dynamically from channel names.
    Batches 256 rows before flushing.  Properly integrated into `flush()` and
    `close()` lifecycle.
  - `SessionWriter::push_fnirs()` dispatches to CSV, Parquet, or both based on
    the user's `storage_format` setting.
  - Session runner: `DeviceEvent::Fnirs` is written to disk and broadcast as a
    `FnirsSample` WebSocket event.

- **Firmware version shown live on the dashboard**: the connected-device
  info row (below the device name) now includes `fw <version>` when the
  Muse sends its firmware version string via a Control JSON event, e.g.
  `fw 3.4.5`.  Previously the value was extracted and stored in
  `state.status.firmware_version` but never surfaced in the UI.

- **`firmware_version` and `bootloader_version` added to the TypeScript
  `DeviceStatus` type**: both fields were serialised by Tauri and arrived
  in every status payload but were absent from the interface declaration in
  `types.ts`, so the TypeScript compiler would silently discard them.

- **Session sidecar (`exg_*.json`) now includes `firmware_version` and
  `serial_number`**: the `Pipeline` struct stores both values when
  `DeviceEvent::Connected` arrives and updates `firmware_version` again
  when the Muse's Control JSON event fires (which arrives a few seconds
  after connect, after the initial `DeviceEvent::Connected`).
  `write_session_meta()` in `shared.rs` now writes both fields.

### Refactor

- **`is_known_eeg_ble_name` unit tests** (`skill-daemon`): 11 cases covering
  all accepted EEG device families (Muse, Ganglion, MW75, Hermes, Mendi,
  IDUN, BrainBit, Unicorn) and 5 rejection cases (JBL, Apple Watch, iPhone,
  AirPods, empty string, anonymous UUID names).  `tempfile` added as a
  `[dev-dependencies]` entry for `skill-daemon`.

- **`write_json_atomic` round-trip test**: verifies the file is written with
  the correct content and no stray `.tmp` file is left behind.

### Bugfixes

- **Dead `hardware_version === "p21"` check in `isMuse2` detection**:
  `"p21"` is the Classic Muse startup preset command string, not a
  hardware version identifier.  The Muse adapter never sets
  `hardware_version`, so this condition was permanently `false` dead code.
  Removed; `isMuse2` now relies solely on the advertising name containing
  `"muse-2"` or `"muse 2"`.

- **Unreachable `_ => {}` arm in session runner event loop**: after adding
  the `DeviceEvent::Meta` handler, all seven `DeviceEvent` variants were
  explicitly matched, making the catch-all arm unreachable (Rust warned
  about it).  Removed.
