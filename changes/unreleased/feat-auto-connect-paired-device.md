### Features

- **Auto-connect to paired devices**: When the BLE scanner discovers a previously paired device while the app is idle (disconnected, no active session or pending reconnect), a session is automatically started. A 30-second cooldown after the last disconnect prevents tight reconnect loops when a device is in BLE range but not responding to connections.
