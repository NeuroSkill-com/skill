### Features

- **Scanner backend settings**: New "Scanner Backends" section in the Devices tab with toggles for each transport (BLE, USB Serial, Emotiv Cortex). Changes are persisted in `settings.json` and take effect on next app restart.

- **Emotiv Cortex connection indicator**: The Cortex scanner toggle shows a live "Connected to Cortex" / "Not connected" badge based on whether Emotiv devices have been discovered via the Cortex WebSocket API.

- **Device log viewer**: New collapsible "Device Log" panel in the Devices tab showing a live, color-coded log of scanner and session events (BLE discovery, USB detection, Cortex polling, connect/disconnect, watchdog). Auto-refreshes every 3 seconds. Entries are kept in a 200-entry ring buffer.

- **Transport badge on devices**: Discovered devices now show a transport badge (USB, Cortex, WiFi) next to their name when they were found via a non-BLE transport.

### Refactor

- **Scanner log tag**: Added `scanner` subsystem to the logging system (`LogConfig`) so scanner events can be toggled independently from `bluetooth` session events.
