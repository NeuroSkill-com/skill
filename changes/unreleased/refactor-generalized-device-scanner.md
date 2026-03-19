### Refactor

- **Generalized device scanner**: Replaced `ble_scanner.rs` with `device_scanner.rs` — a unified background scanner that runs multiple transport-specific backends in parallel:
  - **BLE** — discovers Muse, MW75, Hermes, Ganglion, IDUN devices (existing logic, unchanged)
  - **USB serial** — polls for OpenBCI Cyton/CytonDaisy dongles by detecting FTDI FT231X USB VID/PID and common port patterns (ttyUSB, usbserial), every 5 seconds
  - **Cortex WebSocket** — checks for Emotiv headsets via the local EMOTIV Launcher service (`wss://localhost:6868`), every 10 seconds; only polls when Emotiv credentials are configured
  - All backends share the same auto-connect logic: if a paired device is discovered while idle, connect automatically

- **Transport tag on discovered devices**: `DiscoveredDevice` now carries a `transport` field (`ble`, `usb_serial`, `wifi`, `cortex`) so the UI can display a transport badge. The transport is inferred from device ID prefixes (`usb:`, `cortex:`, or BLE by default).
