### Bugfixes

- **Device routing missed "neurable" and "ige" prefixes**: `detect_device_kind` in `lifecycle.rs` had its own copy of device-name matching that was out of sync with `DeviceKind::from_name`. A Neurable headphone would be routed to the Muse connect path, and an IGE-prefixed IDUN Guardian would also fall through to Muse. Refactored to delegate to the canonical `DeviceKind::from_name` — single source of truth.
- **Emotiv auto-detects actual channel count**: `EmotivAdapter` now detects the real channel count from the first EEG packet. Previously it always assumed EPOC (14 channels); connecting an Insight (5-ch) or MN8 (2-ch) would produce misaligned EEG frames with wrong channel counts.

### Refactor

- **`detect_device_kind` delegates to `DeviceKind::from_name`**: Eliminated duplicated device-name matching constants in `lifecycle.rs`. All device-name detection now flows through `skill-data::device::DeviceKind::from_name`.
