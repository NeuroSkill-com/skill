### Features

- **LSL dashboard integration**: The dashboard now shows connected state,
  signal quality, live EEG channel values, band powers, and waveforms for
  LSL streams — previously only BLE devices updated the dashboard.
- **Generic device badge**: Replaced 6 device-specific badge branches with
  a single badge showing channel count, sample rate, and transport type.
- **Collapsible signal quality**: For high channel counts (>8), the quality
  grid collapses behind a summary row (e.g. "32✓ 0~") to save space.
- **LSL fast resolve**: Named LSL streams now connect in ~500 ms instead of
  waiting the full 5-second discovery timeout.
- **Dynamic channel support**: EEG waveforms, band power chart, and channel
  grid now support any channel count (2–1024+) with a 64-channel rendering
  cap for performance.
- **Disconnected view**: Added "LSL / Settings" button alongside "Scan for
  Device" for quick access to LSL configuration.

### Bugfixes

- **Dashboard showed "DISCONNECTED" for daemon-managed sessions**: The Tauri
  status mirror was overwriting the daemon's authoritative state.  Introduced
  `emit_status_from_daemon()` to prevent mirror-back when data originates
  from the daemon.
- **Tray icon never updated for LSL sessions**: `refresh_tray()` was not
  called from the daemon status poll path.
- **LSL connect button re-enabled instantly**: The button now stays disabled
  with a "Connecting…" spinner until the daemon confirms the session state.
- **Band chart empty for >12 channels**: Fixed hardcoded `MAX_CH=12` buffer
  limit in BandChart; now dynamically sized.
- **EEG waveform capped at 12 channels**: Frontend `EEG_CHANNELS` constant
  updated from 12 to 32; chart buffers now sized from the actual channel
  count prop.
- **`StatusResponse` missing device fields**: Expanded with channel names,
  sample rate, signal quality, device identity, and capability flags so the
  daemon can fully describe the connected device.

### Refactor

- **Central `apply_daemon_status()` helper**: Replaced 4 copy-paste
  cherry-pick sites with a single function that maps all `StatusResponse`
  fields to `DeviceStatus`.
- **`StatusResponse::clear_device()`**: Clean disconnect reset instead of
  manual field clearing.
- **BLE-centric i18n strings**: Updated to be device-agnostic (BCI headset
  → device, added LSL references).
