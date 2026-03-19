### Features

- **Auto-connect to paired devices**: When the BLE scanner discovers a previously paired device while the app is idle (disconnected, no active session or pending reconnect), a session is automatically started. No cooldown is needed — `start_session()` immediately marks the app as connecting, preventing duplicate attempts, and the normal retry backoff handles failures.
