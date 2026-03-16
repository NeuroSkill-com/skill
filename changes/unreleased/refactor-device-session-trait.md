### Refactor

- **Unified device session via `DeviceAdapter` trait**: Replaced four copy-pasted session modules (`muse_session.rs`, `mw75_session.rs`, `openbci_session.rs`, `hermes_session.rs` — 2,070 lines) with a trait-based architecture (1,970 lines). Added `DeviceAdapter` async trait, unified event types (`DeviceEvent`, `EegFrame`, `PpgFrame`, `ImuFrame`, `BatteryFrame`), and capability flags (`DeviceCaps`) to `skill-devices::session`. Each device has a small adapter (107–223 lines) that translates vendor events into the common vocabulary. A single generic event loop in `session_runner.rs` handles DSP, CSV, DND, battery, and emit for all devices.

### Bugfixes

- **DND focus mode now works on all devices**: OpenBCI and Hermes sessions were missing the Do Not Disturb tick logic (only Muse and MW75 had it). The shared `session_runner` now runs DND for every device that produces EEG band snapshots.

- **Battery alerts use `BatteryEma` from `skill-devices`**: Replaced two inline EMA implementations (Muse, MW75) with the existing `BatteryEma` struct, ensuring consistent smoothing and alert thresholds across devices.

### Build

- **Added `tokio`, `async-trait`, `bitflags` deps to `skill-devices`**: Only `tokio/sync` (channel primitives) is used — no runtime. Added `tokio-util` to `src-tauri` for `CancellationToken`.
