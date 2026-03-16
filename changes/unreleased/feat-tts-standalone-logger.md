### Features

- **Standalone TTS logger**: Added `skill_tts::log` module with pluggable callback sink (`set_log_callback`) and `tts_log!` macro. All `eprintln!("[tts] ...")` / `eprintln!("[neutts] ...")` calls in `kitten.rs`, `neutts.rs`, and `lib.rs` now route through the unified logger. On the Tauri side, `tts::init_tts_logger()` wires TTS output through `SkillLogger` so the `tts` subsystem toggle in log config controls TTS log visibility. Logging is enabled by default and can be toggled at runtime via `set_log_enabled`.
