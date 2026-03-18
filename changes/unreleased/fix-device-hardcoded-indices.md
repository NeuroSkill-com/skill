### Bugfixes

- **FAA uses name-based electrode lookup**: Frontal Alpha Asymmetry in `eeg_bands.rs` now resolves left/right frontal electrodes by 10-20 name instead of hardcoded indices [1]/[2]. Previously, non-Muse devices computed FAA from wrong electrodes (e.g. Emotiv used F7/F3 — both left hemisphere).
- **Cognitive load uses name-based electrode lookup**: `compute_cognitive_load` now finds frontal (theta) and parietal (alpha) electrodes by 10-20 name prefix instead of assuming Muse's 4-channel index layout. Falls back to index-based split for generic labels.
- **Laterality index uses name-based hemisphere detection**: `laterality_index_fn` now determines left/right hemisphere from 10-20 naming convention (odd=left, even=right) instead of hardcoded indices [0..1] vs [2..3]. Previously, MW75 computed laterality from 4 left-hemisphere channels only.
- **IDUN battery not shown in UI**: Added `isIdun` to the `hasBattery` derived flag in `+page.svelte` so the battery indicator renders for IDUN Guardian (which reports battery via BLE).
- **Ganglion showed Muse electrode labels**: Added `GANGLION_CH`/`GANGLION_COLOR` constants and wired them into the dashboard channel-label selector. Previously, connecting a Ganglion displayed Muse names (TP9/AF7/AF8/TP10) instead of generic Ch1–Ch4.
- **Hermes channel labels in constants.ts**: Updated `HERMES_CH` from generic `Ch1–Ch8` to proper 10-20 names matching Rust `HERMES_CHANNEL_NAMES`.
- **Emotiv constants.ts channel count/names**: Updated `EMOTIV_EEG_CHANNELS` from 12 to 14, added missing `F8`/`AF4` to `EMOTIV_CH` and corresponding colours.
- **Misleading Ganglion sample-rate comment**: Corrected doc comment on `MUSE_SAMPLE_RATE` — Ganglion uses 200 Hz, not 256 Hz.
