### Bugfixes

- **Emotiv disconnect cleanup**: when an Emotiv headset disconnects before any EEG data is recorded (e.g. powered off right after connecting, or Cortex session interrupted), the app now properly transitions to the disconnected state. Previously the UI would stay stuck on "connected" because `go_disconnected` was only called when the CSV recording file had been opened — which requires at least one EEG frame.
- **Disconnect events for all break paths**: the data watchdog timeout and event-channel-closed paths in the session runner now call `on_disconnected` to emit the `device-disconnected` event and toast, consistent with the explicit `DeviceEvent::Disconnected` path.
- **Emotiv headset disconnect/failure warnings**: the adapter now handles Cortex warning codes 102 (HEADSET_DISCONNECTED) and 103 (HEADSET_CONNECTION_FAILED) in addition to codes 0 and 1. This triggers an immediate disconnect instead of waiting for the 15-second data watchdog timeout.

### Dependencies

- **emotiv**: bumped to 0.0.9 — adds `HEADSET_DISCONNECTED` (102) and `HEADSET_CONNECTION_FAILED` (103) warning constants; emits `CortexEvent::Disconnected` immediately when either is received.
