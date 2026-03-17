### Features

- **Emotiv device support**: Added `EmotivAdapter` for Emotiv EPOC X, EPOC+, Insight, and Flex headsets via the Cortex WebSocket API (JSON-RPC 2.0). Streams EEG (up to 14 ch @ 128 Hz), motion/IMU, and battery data through the unified session runner.
- **IDUN Guardian device support**: Added `IdunAdapter` for the IDUN Guardian in-ear EEG earbud over BLE. Streams single-channel bipolar EEG (1 ch @ 250 Hz), 6-DOF IMU (accelerometer + gyroscope), and battery data.
- **Device constants**: Added hardware constants for Emotiv (EPOC 14-ch, Insight 5-ch, 128 Hz sample rate, channel labels) and IDUN Guardian (1-ch, 250 Hz, channel label) to `skill-constants`.
- **DeviceKind::Idun**: Extended `DeviceKind` enum in `skill-data` with the `Idun` variant, capability flags, and name-based detection (`idun`, `ige`, `guardian` prefixes).
- **TypeScript device layer**: Added `"idun"` to the `DeviceKind` union and `IDUN_CAPS` capability table in `device.ts` for UI-side device detection.
