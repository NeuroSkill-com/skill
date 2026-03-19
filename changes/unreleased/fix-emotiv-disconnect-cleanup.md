### Bugfixes

- **Emotiv disconnect cleanup**: when an Emotiv headset disconnects before any EEG data is recorded (e.g. powered off right after connecting, or Cortex session interrupted), the app now properly transitions to the disconnected state. Previously the UI would stay stuck on "connected" because `go_disconnected` was only called when the CSV recording file had been opened — which requires at least one EEG frame.
- **Disconnect events for all break paths**: the data watchdog timeout and event-channel-closed paths in the session runner now call `on_disconnected` to emit the `device-disconnected` event and toast, consistent with the explicit `DeviceEvent::Disconnected` path.
