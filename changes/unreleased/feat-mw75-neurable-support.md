### Features

- **Neurable MW75 Neuro headphone support**: Added full session support for the Master & Dynamic MW75 Neuro EEG headphones (12 channels at 500 Hz). The `mw75` crate is added to `skill-devices` and re-exported. Connection follows the `mw75` CLI binary lifecycle: BLE discover → connect → activate (EEG + raw mode) → disconnect BLE → RFCOMM stream on channel 25. Headphones must first be paired via OS Bluetooth Settings (hold power button 4+ seconds). BLE scanner discovers MW75 by name and GATT service UUID. RFCOMM transport is behind the `mw75-rfcomm` feature flag (disabled by default) because linking IOBluetooth.framework on macOS adds ~2 s to process startup. Without it, EEG data arrives via BLE GATT notifications. DSP pipeline processes first 4 of 12 channels; all written to CSV. Battery, DND, band enrichment fully integrated.

### Performance

- **MW75 RFCOMM feature flag**: The `mw75-rfcomm` Cargo feature is opt-in to avoid linking IOBluetooth.framework on macOS, which `dyld` loads at process launch adding ~2 s startup latency. Enable with `--features mw75-rfcomm` when RFCOMM streaming is needed.
