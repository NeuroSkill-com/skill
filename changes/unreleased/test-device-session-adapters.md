### Refactor

- **Added 25 tests for `skill-devices::session` adapters**: Covers channel accumulation (Muse alignment of per-electrode delivery, partial/complete frames, out-of-range electrodes), event translation (EEG, PPG, IMU, battery, connected, disconnected, activation-skipped, packets-dropped-skipped), synthetic connected injection (MW75 RFCOMM, OpenBCI), capability flags, descriptor construction, and pipeline channel capping. Total: 34 tests in `skill-devices` (up from 9).

- **Made adapter handle fields `Option`-based for testability**: `Mw75Adapter` and `HermesAdapter` handle fields are now `Option<Handle>` with `new_for_test()` constructors that pass `None`, avoiding unsafe `MaybeUninit` hacks. `OpenBciAdapter` gained `from_receiver()` for direct async channel injection without needing private `StreamHandle` fields.
