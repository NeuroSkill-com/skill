### Bugfixes

- **Emotiv subscribe race condition**: `connect_emotiv` now waits for the `SessionCreated` event before calling `subscribe`. Previously it called `subscribe` immediately after `client.connect()`, but `connect()` only opens the WebSocket — the auth flow (hasAccessRight → authorize → queryHeadsets → createSession) runs asynchronously. The subscribe was sent with an empty cortexToken and session ID, causing an immediate `-32014 Cortex token is invalid` error.

- **Emotiv session stability**: Upgraded to emotiv crate v0.0.4 which prevents `ACCESS_RIGHT_GRANTED`, `HEADSET_CONNECTED`, and `HEADSET_SCANNING_FINISHED` warning handlers from re-authorizing or re-querying headsets when a session is already active.

- **Emotiv scanner is now side-effect-free**: The Cortex scanner probe only authorizes — it does NOT send `queryHeadsets` or `getCortexInfo`. The scanner also skips polling entirely when a session is active or a reconnect is pending, and waits 5 seconds at startup to avoid racing with the auto-connect flow.

- **Emotiv auto-connect without pairing**: Cortex-discovered and USB-discovered devices are now treated as trusted transports and auto-connect when the app is idle, without requiring manual pairing first. BLE devices still require pairing as before (since BLE advertisements can come from any nearby device).

- **Emotiv reconnect uses correct device ID**: `start_session` now pins the scanner-level device ID (e.g. `"cortex:emotiv"`) into `status.device_id` before the adapter runs. This ensures `on_connected` pairs the device with the correct ID (instead of the Cortex session ID), and reconnect retries route to `connect_emotiv` via the `cortex:` prefix.

- **Device kind routing by ID prefix**: `detect_device_kind` now checks the device ID prefix (`cortex:` → emotiv, `usb:` → ganglion) before falling back to name-based detection. This ensures Cortex-discovered devices route to `connect_emotiv` regardless of their headset ID format.
