### Bugfixes

- **Emotiv scanner no longer kills active sessions**: The Cortex scanner now uses `auto_create_session: false` and manually sends `queryHeadsets` after authorization. Previously `auto_create_session: true` caused the scanner to create a Cortex session on every 10-second poll, which made the Cortex service stop streams on the real session — disconnecting the headset immediately after connecting. The scanner also skips polling entirely when a session is active or a reconnect is pending.

- **Emotiv device name resolution**: The Cortex scanner now retrieves the real headset ID (e.g. "INSIGHT-5AF2C39E", "EPOCX-ABCDEF12") from the Cortex API after authorization instead of using a hardcoded synthetic name.

- **Emotiv auto-connect without pairing**: Cortex-discovered and USB-discovered devices are now treated as trusted transports and auto-connect when the app is idle, without requiring manual pairing first. BLE devices still require pairing as before (since BLE advertisements can come from any nearby device).

- **Device kind routing by ID prefix**: `detect_device_kind` now checks the device ID prefix (`cortex:` → emotiv, `usb:` → ganglion) before falling back to name-based detection. This ensures Cortex-discovered devices route to `connect_emotiv` regardless of their headset ID format.
