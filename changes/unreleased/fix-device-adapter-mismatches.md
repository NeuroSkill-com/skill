### Bugfixes

- **Emotiv TS channel count/names**: Fixed `EMOTIV_CAPS` in `device.ts` — `channelCount` was 12 instead of 14, and electrode names were missing `"F8"` and `"AF4"` (EPOC X/EPOC+ have 14 channels).
- **MW75 Rust detection missing "neurable"**: Added `n.contains("neurable")` to `DeviceKind::from_name` so devices advertising as "Neurable-XYZ" are correctly identified as MW75, matching the TypeScript detection logic.
- **Hermes TS electrode names**: Replaced generic `["Ch1",...,"Ch8"]` with proper 10-20 names `["Fp1","Fp2","AF3","AF4","F3","F4","FC1","FC2"]` to match the Rust constants in `skill-constants`.
- **IDUN adapter META capability**: Added `DeviceCaps::META` to `IdunAdapter` caps, since the adapter emits `DeviceEvent::Meta` for `GuardianEvent::DeviceInfo` but was not declaring the capability.
