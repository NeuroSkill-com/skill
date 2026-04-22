## ANT Neuro eego amplifier support

### Features

- Add ANT Neuro eego amplifier family support via the `antneuro` crate (native USB backend, no vendor SDK required)
- Supported models: eego 8, eego 24, eego 64, eego mylab, eego sport, eego rt, eego hub
- Auto-detect channel count and sample rate from the amplifier on first data block
- Configurable cap layout via Device API settings (`antneuro_cap`: auto, 10-20, 10-10, or explicit channel count)
- Configurable sample rate via Device API settings (`antneuro_sample_rate`: 500, 512, 1000, 2048, etc.)
- Waveguard electrode name mappings for 8, 21, 24, 25, 32, and 64-channel cap configurations
- USB scanner auto-detects connected eego amplifiers every other tick
- Device images and company logo for all 7 amplifier models in the Supported Devices catalog

### UI

- Improved supported devices card grid: wider columns, taller images with `object-contain`, text wraps instead of truncating

### Files changed

- `crates/skill-devices/src/session/antneuro.rs` — new `AntNeuroAdapter` implementing `DeviceAdapter`
- `crates/skill-devices/src/session/mod.rs` — register antneuro module
- `crates/skill-devices/Cargo.toml` — add `antneuro` dependency (native feature)
- `crates/skill-daemon/Cargo.toml` — add `antneuro` dependency (native feature)
- `crates/skill-daemon/src/session/connect.rs` — add `AntNeuro` connect route and predicates
- `crates/skill-daemon/src/session/connect_wired.rs` — add `connect_antneuro()` with settings integration
- `crates/skill-daemon/src/scanner.rs` — add `detect_antneuro_devices()` USB scanner
- `crates/skill-settings/src/lib.rs` — add `antneuro_sample_rate` and `antneuro_cap` settings
- `crates/skill-data/src/device.rs` — add `AntNeuro` to `DeviceKind` enum, capabilities, and supported companies catalog
- `src/lib/devices-logic.ts` — add antneuro/eego device image mapping
- `src/lib/DevicesTab.svelte` — add antneuro/eego device image mapping; improve device card grid layout
- `src/lib/i18n/en/settings.ts` — add i18n strings for ANT Neuro company, 7 devices, and setup instructions
- `static/logos/antneuro.jpg` — company logo
- `static/devices/antneuro-eego-8.jpg` — eego 8 product image
- `static/devices/antneuro-eego-24.jpg` — eego 24 product image
- `static/devices/antneuro-eego-64.webp` — eego 64 product image
- `static/devices/antneuro-eego-mylab.webp` — eego mylab product image
- `static/devices/antneuro-eego-sport.webp` — eego sport product image
- `static/devices/antneuro-eego-rt.webp` — eego rt product image
- `static/devices/antneuro-eego-hub.png` — eego hub product image
