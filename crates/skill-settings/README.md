# skill-settings

Persistent configuration types and disk I/O for NeuroSkill.

## Overview

Owns the entire user-facing settings surface: serialization/deserialization of the JSON settings file, default values for every field, and helper functions for locating paths. The `Settings` struct is the single source of truth read at startup and written on every user change.

## Modules

| Module | Description |
|---|---|
| `lib.rs` | All settings types, defaults, load/save logic |
| `screenshot_config` | `ScreenshotConfig` — screenshot capture/embed/OCR/GIF settings, extracted here so that the heavy `skill-screenshots` crate is not needed just to read/write configuration |

## Key types

| Type | Description |
|---|---|
| `UserSettings` | Top-level settings: appearance (theme, accent), shortcuts, EEG model, filter config, screenshot config, TTS config, LLM config, calibration, hooks, sleep, DND, device API, and more |
| `OpenBciBoard` | Enum of supported OpenBCI boards (Ganglion, Cyton, Daisy) with channel count, sample rate, and interface queries |
| `OpenBciConfig` | Serial/Wi-Fi port, board selection, channel names |
| `DeviceApiConfig` | Configuration for third-party device APIs (Emotiv Cortex client ID/secret) |
| `SleepPreset` / `SleepConfig` | Sleep tracking presets (Short / Normal / Long / Custom) and configuration (target minutes, alarm) |
| `UmapUserConfig` | UMAP hyperparameters (neighbours, min-distance, metric) |
| `CalibrationProfile` / `CalibrationConfig` | Calibration action lists and profile management |
| `HookRule` / `HookLastTrigger` / `HookStatus` | User-defined automation rules triggered by label similarity, with trigger history and status tracking |
| `DoNotDisturbConfig` | DND focus-mode configuration (thresholds, delays) |
| `ScreenshotConfig` | Screenshot capture interval, image size, quality, embed backend, OCR engine, GPU toggle, GIF settings |

## Key functions

| Function | Description |
|---|---|
| `default_skill_dir()` | Platform-appropriate data directory |
| `settings_path(skill_dir)` | Path to `settings.json` |
| `tilde_path(p)` | Replace `$HOME` with `~` for display |
| `load_umap_config` / `save_umap_config` | Read/write UMAP settings |
| `default_*` functions | Default values for every settings field |

## Feature flags

| Flag | Description |
|---|---|
| `llm` | Enables the `chat_shortcut` field on `Settings` |

## Dependencies

- `skill-constants` — default values and file names
- `skill-eeg` — `FilterConfig`, `EegModelConfig`
- `skill-tts` — `NeuttsConfig`
- `skill-llm` — `LlmConfig`
- `skill-data` — `PairedDevice`
- `serde` / `serde_json` — JSON serialization
- `dirs` — platform directories
