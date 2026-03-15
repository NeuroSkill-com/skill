# skill-devices

Device-session pure logic — composite EEG scores, battery monitoring, and DND focus-mode engine.

## Overview

Encapsulates the deterministic, side-effect-free algorithms that drive a live EEG session: deriving high-level cognitive scores from band powers, smoothing battery readings, and deciding when to toggle the OS Do Not Disturb mode. Zero Tauri dependencies — every function is a pure computation suitable for unit testing and reuse in CLI tools.

## Composite scores

| Function | Description |
|---|---|
| `compute_meditation` | Alpha/beta ratio, stillness, and optional HRV (RMSSD) → 0–100 |
| `compute_cognitive_load` | Frontal-theta / parietal-alpha sigmoid → 0–100 |
| `compute_drowsiness` | Theta-alpha ratio + alpha-spindle detection → 0–100 |
| `compute_engagement_raw` | Beta / (alpha + theta) ratio |
| `focus_score` | Sigmoid mapping of raw engagement to 0–100 |

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

- `skill-eeg` — `BandSnapshot` type
- `serde` / `serde_json` — serialization
