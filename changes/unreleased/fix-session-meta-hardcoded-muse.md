### Bugfixes

- **Session metadata hardcoded Muse values**: `write_session_meta` wrote `"sample_rate_hz": 256`, `"channels": ["TP9","AF7","AF8","TP10"]`, and `"channel_count": 4` for all devices. Now uses actual device values from `DeviceStatus` (set at session start). Recordings from MW75, Emotiv, Hermes, IDUN, and Ganglion had incorrect metadata in the JSON sidecar file.
- **Ganglion connected at wrong sample rate**: `connect_ganglion` passed `EEG_SAMPLE_RATE` (256 Hz) to `OpenBciAdapter::make_descriptor`, but Ganglion runs at 200 Hz. Added `GANGLION_SAMPLE_RATE` (200 Hz) constant and use it instead. This caused the entire DSP pipeline (filter, band analyzer, artifact detector) to use 256 Hz for a 200 Hz device.
- **Missing constants in prelude**: Added `GANGLION_SAMPLE_RATE`, `GANGLION_CHANNEL_NAMES`, `HERMES_*`, and `MW75_*` constants to the `skill-constants` prelude for easier access across crates.
