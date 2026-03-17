### UI

- **Hide charts when no session is active**: Band Powers and EEG Waveforms charts are now hidden on the main dashboard when the device is not connected, reducing visual clutter in the disconnected/scanning states.
- **Show PPG/IMU only for capable devices**: PPG charts, PPG metrics, Head Pose card, and IMU chart are now gated by `deviceCaps.hasPpg` / `deviceCaps.hasImu` so they only appear for devices that actually have those sensors.
