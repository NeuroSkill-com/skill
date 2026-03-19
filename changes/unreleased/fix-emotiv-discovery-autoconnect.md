### Bugfixes

- **Emotiv scanner is now side-effect-free**: The Cortex scanner probe only authorizes — it does NOT send `queryHeadsets` or `getCortexInfo`. The emotiv crate's internal `queryHeadsets` handler automatically calls `connect_headset`/`create_session`, which was killing the active session on every 10-second poll. The scanner also skips polling entirely when a session is active or a reconnect is pending.

- **Emotiv auto-connect without pairing**: Cortex-discovered and USB-discovered devices are now treated as trusted transports and auto-connect when the app is idle, without requiring manual pairing first. BLE devices still require pairing as before (since BLE advertisements can come from any nearby device).

- **Emotiv reconnect uses correct device ID**: `start_session` now pins the scanner-level device ID (e.g. `"cortex:emotiv"`) into `status.device_id` before the adapter runs. This ensures `on_connected` pairs the device with the correct ID (instead of the Cortex session ID), and reconnect retries route to `connect_emotiv` via the `cortex:` prefix.

- **Device kind routing by ID prefix**: `detect_device_kind` now checks the device ID prefix (`cortex:` → emotiv, `usb:` → ganglion) before falling back to name-based detection. This ensures Cortex-discovered devices route to `connect_emotiv` regardless of their headset ID format.
