### Bugfixes

- **Emotiv headset disconnect detection**: The Emotiv adapter now translates Cortex API warning codes `CORTEX_STOP_ALL_STREAMS` (0) and `CORTEX_CLOSE_SESSION` (1) into `DeviceEvent::Disconnected`, giving the session runner instant notification when a headset goes away instead of waiting up to 15 seconds for the data watchdog to fire. `CortexEvent::Error` is also surfaced as a disconnect to trigger immediate reconnection.

### Features

- **Emotiv adapter test coverage**: Added tests for EEG translation, headset disconnect on stop-all-streams/close-session warnings, error-to-disconnect mapping, and warning filtering.
