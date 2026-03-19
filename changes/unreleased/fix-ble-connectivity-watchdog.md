### Bugfixes

- **Data watchdog for silent BLE disconnects**: Added a 15-second data watchdog to the session event loop. If no device event arrives within the timeout, the connection is treated as silently lost and auto-reconnect is triggered. This catches scenarios where the BLE link stays alive but GATT notifications stop flowing (radio interference, device sleep, firmware hang).

- **Reconnect retry limit**: Auto-reconnect now gives up after 12 consecutive failed attempts (~51 seconds of total backoff) instead of retrying indefinitely. A toast notification informs the user to reconnect manually when the limit is reached. This prevents draining battery on a device that was intentionally turned off or moved out of range.
