### Bugfixes

- **Emotiv/IDUN channel labels defaulted to Muse 4-ch**: Dashboard channel labels and colors now correctly show 14 Emotiv EPOC channels or 1 IDUN Guardian channel instead of falling back to the Muse TP9/AF7/AF8/TP10 labels.
- **ElectrodePlacement SVG missing Emotiv & IDUN**: Added 14-electrode Emotiv EPOC and single-electrode IDUN Guardian presets to the 2D electrode placement diagram with correct 10-20 positions.
- **Device info badge only shown for Ganglion**: Non-Muse device info badges (channel count, sample rate) now also appear for Emotiv, IDUN, and Hermes connections.
- **EEG expanded grid always 2-column**: The expanded EEG channel grid now adapts columns to the channel count (2 for ≤4ch, 3 for 5-8ch, 4 for >8ch) instead of being hardcoded to 2 columns except MW75.

### Refactor

- **DeviceKind type updated**: Added missing `"ganglion"`, `"mw75"`, and `"hermes"` variants to the TypeScript `DeviceKind` union so it matches all backend device kinds.
- **Stale JSDoc**: Updated `device_kind` field comment in `types.ts` to reference `DeviceKind` instead of listing an incomplete set.
