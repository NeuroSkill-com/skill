# skill-settings

Persistent configuration types and disk I/O for NeuroSkill.

## Overview

Owns the entire user-facing settings surface: serialization/deserialization of the JSON settings file, default values for every field, and helper functions for locating paths. The `Settings` struct is the single source of truth read at startup and written on every user change.

## Key types

| Type | Description |
|---|---|
| `Settings` (re-exported) | Top-level settings: appearance (theme, accent), shortcuts, EEG model, filter config, screenshot config, TTS config, LLM config, calibration, hooks, and more |
| `OpenBciBoard` | Enum of supported OpenBCI boards (Ganglion, Cyton, Daisy) with channel count, sample rate, and interface queries |
| `OpenBciConfig` | Serial/Wi-Fi port, board selection, channel names |
| `UmapUserConfig` | UMAP hyperparameters (neighbours, min-distance, metric) |
| `CalibrationProfile` / `CalibrationConfig` | Calibration action lists and profile management |
| `HookRule` | User-defined automation rule triggered by label similarity |

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
- `skill-screenshots` — `ScreenshotConfig`
- `skill-data` — `PairedDevice`
- `serde` / `serde_json` — JSON serialization
- `dirs` — platform directories
