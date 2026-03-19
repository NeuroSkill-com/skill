# skill-data

Pure data types and utility modules for NeuroSkill.

## Overview

Houses the shared data layer: SQLite-backed stores, device descriptors, DND (Do Not Disturb) integration, GPU telemetry, and miscellaneous helpers that multiple crates depend on but that carry no Tauri dependency.

## Modules

| Module | Description |
|---|---|
| `device` | `PairedDevice`, `DeviceKind` enum (Muse / Ganglion / …), `DeviceCapabilities` with channel count, sample rate, BLE/serial/Wi-Fi flags |
| `label_store` | `LabelStore` — SQLite CRUD for user labels (insert, list, update, recent, count) |
| `screenshot_store` | Screenshot metadata storage and retrieval |
| `activity_store` | `ActivityStore` — tracks active-window info and keyboard/mouse input activity with 5-minute bucketing |
| `hooks_log` | `HooksLog` — SQLite audit log for hook-rule firings |
| `health_store` | `HealthStore` — SQLite store for Apple HealthKit data synced from a companion iOS app (sleep samples, workouts, heart rate, steps, mindfulness, generic health metrics). Idempotent upsert by `(source_id, start_utc, end_utc)`. |
| `active_window` | `ActiveWindowInfo` struct for the currently focused window |
| `dnd` | macOS Focus Mode helpers: `query_os_active`, `set_dnd`, `list_focus_modes` |
| `gpu_stats` | `GpuStats` — cross-platform GPU utilization/VRAM reader (macOS `powermetrics`, Linux `nvidia-smi`, Windows NVML) |
| `ppg_analysis` | PPG (photoplethysmography) signal analysis |
| `session_csv` | CSV import/export for recording sessions |
| `session_parquet` | Parquet recording writer — columnar Apache Parquet files with Snappy compression as a drop-in alternative to CSV. Produces `exg_<ts>.parquet`, `exg_<ts>_ppg.parquet`, `exg_<ts>_metrics.parquet`. |
| `session_writer` | Unified session writer — `SessionWriter` enum that dispatches to either `CsvState` or `ParquetState` based on `StorageFormat` setting |
| `util` | Miscellaneous shared helpers |

## Dependencies

- `skill-constants`, `skill-eeg` — shared constants and EEG types
- `rusqlite` — SQLite storage
- `serde` / `serde_json` — serialization
- `csv` — CSV reading/writing
- `parquet` / `arrow-array` / `arrow-schema` — Parquet columnar storage
- `llmfit-core` — lightweight model fitting
- `sysinfo`, `libc` — system information
