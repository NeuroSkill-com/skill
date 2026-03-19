# skill-devices

Device-session pure logic — composite EEG scores, battery monitoring, DND focus-mode engine, and unified device adapter abstraction.

## Overview

Encapsulates the deterministic, side-effect-free algorithms that drive a live EEG session: deriving high-level cognitive scores from band powers, smoothing battery readings, deciding when to toggle the OS Do Not Disturb mode, and providing a unified `DeviceAdapter` trait that each hardware driver implements. Zero Tauri dependencies — every function is a pure computation suitable for unit testing and reuse in CLI tools.

## Device adapter system

### Capability model

Instead of compile-time feature flags, each adapter declares its `DeviceCaps` at construction time. The session runner inspects caps to decide which event types to expect, which CSV columns to create, and whether PPG / IMU visualization should be enabled.

| Type | Description |
|---|---|
| `DeviceCaps` | Bitflags: `EEG`, `PPG`, `IMU`, `BATTERY`, `META` |
| `DeviceDescriptor` | Static device info: name, channel count, sample rate, channel names |
| `DeviceInfo` | Runtime info: firmware version, serial number |
| `DeviceEvent` | Unified event enum: `Eeg(EegFrame)`, `Ppg(PpgFrame)`, `Imu(ImuFrame)`, `Battery(BatteryFrame)`, `Info(DeviceInfo)`, `Disconnected` |
| `DeviceAdapter` | Async trait — `caps()`, `descriptor()`, `next_event()`, `shutdown()` |

### Frame types

| Type | Description |
|---|---|
| `EegFrame` | Timestamp + multi-channel EEG samples |
| `PpgFrame` | Timestamp + PPG optical data |
| `ImuFrame` | Timestamp + accelerometer/gyroscope/magnetometer |
| `BatteryFrame` | Battery level, charging status, temperature |

### Adapter implementations

| Adapter | Device | Channels | Sample rate | Capabilities |
|---|---|---|---|---|
| `muse::MuseAdapter` | Muse 2 / Muse S | 4 ch | 256 Hz | EEG, PPG, IMU, Battery |
| `mw75::Mw75Adapter` | Neurable MW75 Neuro | 12 ch | 500 Hz | EEG, Battery |
| `hermes::HermesAdapter` | Hermes V1 | 8 ch | 250 Hz | EEG, IMU, Battery |
| `openbci::OpenBciAdapter` | Ganglion / Cyton / Galea | 4–24 ch | varies | EEG, Battery |
| `emotiv::EmotivAdapter` | Emotiv EPOC / Insight / Flex | 5–14 ch | 128 Hz | EEG, Battery |
| `idun::IdunAdapter` | IDUN Guardian | 1 ch | 250 Hz | EEG, Battery |

## Composite scores

| Function | Description |
|---|---|
| `compute_meditation` | Alpha/beta ratio, stillness, and optional HRV (RMSSD) → 0–100 |
| `compute_cognitive_load` | Frontal-theta / parietal-alpha sigmoid → 0–100 |
| `compute_drowsiness` | Theta-alpha ratio + alpha-spindle detection → 0–100 |
| `compute_engagement_raw` | Beta / (alpha + theta) ratio |
| `focus_score` | Sigmoid mapping of raw engagement to 0–100 |

## Snapshot enrichment

| Type / Function | Description |
|---|---|
| `SnapshotContext` | Holds stillness, RMSSD, and channel names for enriching a band snapshot |
| `enrich_band_snapshot` | Fills meditation, cognitive load, drowsiness, and focus scores on a `BandSnapshot` |

## Battery EMA

| Type | Description |
|---|---|
| `BatteryEma` | Exponential moving average filter for noisy battery readings |
| `BatteryAlert` | `None` / `Low` / `Critical` threshold alerts |

## DND focus-mode engine

| Item | Description |
|---|---|
| `DndConfig` | Tuning knobs — thresholds, window size, exit delay |
| `DndState` | Rolling-window state for the decision engine |
| `DndDecision` | Output: whether to enable/disable DND this tick |
| `dnd_tick` | Pure function — feeds a new focus score and returns a decision |
| `dnd_apply_os_result` | Updates state after the OS toggle completes |
| `SNR_LOW_DB` / `SNR_LOW_TICKS` | Low-signal detection constants |

## Dependencies

- `skill-constants` — shared constants (sample rates, channel names)
- `skill-eeg` — `BandSnapshot` type
- `skill-data` — device types
- `muse-rs` — Muse 2 / Muse S BLE driver
- `openbci` — OpenBCI board drivers (Ganglion BLE, Cyton serial)
- `mw75` — Neurable MW75 Neuro EEG headphones
- `hermes-ble` — Hermes V1 EEG headset (BLE)
- `emotiv` — Emotiv EPOC / Insight / Flex (Cortex WebSocket API)
- `idun` — IDUN Guardian in-ear EEG earbud (BLE)
- `async-trait` / `tokio` — async adapter trait and channel primitives
- `bitflags` — capability flags
- `serde` / `serde_json` — serialization
