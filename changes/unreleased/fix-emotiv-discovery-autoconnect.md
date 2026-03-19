### Bugfixes

- **Emotiv device name resolution**: The Cortex scanner now retrieves the real headset ID (e.g. "EPOCX-ABCDEF12") from the Cortex API after authorization instead of using a hardcoded synthetic name. The `auto_create_session` flag is set to `true` so the client's internal queryHeadsets flow populates the headset ID.

- **Emotiv auto-connect without pairing**: Cortex-discovered and USB-discovered devices are now treated as trusted transports and auto-connect when the app is idle, without requiring manual pairing first. BLE devices still require pairing as before (since BLE advertisements can come from any nearby device).

- **Device kind routing by ID prefix**: `detect_device_kind` now checks the device ID prefix (`cortex:` → emotiv, `usb:` → ganglion) before falling back to name-based detection. This ensures Cortex-discovered devices route to `connect_emotiv` regardless of their headset ID format.
