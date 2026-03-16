# skill-tts

Text-to-speech engine for NeuroSkill.

## Overview

Provides two pluggable TTS backends — **KittenTTS** (lightweight, fast) and **NeuTTS** (neural, higher quality) — behind a unified API. Handles model downloading from HuggingFace, eSpeak phonemizer initialization, audio playback via `rodio`, voice listing/selection, and graceful shutdown. Backend selection is controlled via feature flags.

## Modules

| Module | Description |
|---|---|
| `config` | `NeuttsConfig` — backbone repo, voice ID, and tuning parameters |
| `log` | Standalone logger with pluggable callback sink and `tts_log!` macro |
| `kitten` | KittenTTS backend: model loading, voice management, speak/stop command channel, audio playback |
| `neutts` | NeuTTS neural backend: model loading, sample caching with SHA-256, wgpu inference |

## Feature flags

| Flag | Description |
|---|---|
| `tts-kitten` | Enable KittenTTS backend (pulls in `kittentts`, `rodio`) |
| `tts-neutts` | Enable NeuTTS backend (pulls in `neutts`, `rodio`, `sha2`, `hound`) |

## Key functions

| Function | Description |
|---|---|
| `log::set_log_callback(cb)` | Install a custom log sink (e.g. route through `SkillLogger`) |
| `log::set_log_enabled(bool)` | Enable / disable TTS log output at runtime |
| `set_logging(bool)` | Legacy alias for `log::set_log_enabled` |
| `init_tts_dirs(dir)` | Create TTS data directories |
| `init_espeak_bundled_data_path` / `init_espeak_data_path` | Set up eSpeak phonemizer data |
| `play_f32_audio(samples, sample_rate)` | Play raw f32 PCM via rodio |
| `tts_list_voices()` | List available voices for the active backend |
| `tts_shutdown()` | Gracefully stop playback and release resources |
| `use_neutts()` | Query which backend is active |
| `neutts_apply_config(cfg)` | Hot-reload NeuTTS settings |

## Key types

| Type | Description |
|---|---|
| `NeuttsConfig` | Neural TTS configuration |
| `TtsProgressEvent` | Progress/status event for UI (step, ready, error, unloaded) |
| `NeuttsVoiceInfo` | Voice metadata (name, sample path, language) |

## Dependencies

- `serde` / `serde_json` — configuration serialization
- `tokio` — async coordination
- `hf-hub` / `dirs` — model downloads and cache paths
- `kittentts` (optional) — KittenTTS engine with eSpeak
- `neutts` (optional) — NeuTTS neural engine (wgpu, Metal)
- `rodio` (optional) — audio playback
- `sha2` / `hound` (optional) — sample caching for NeuTTS
