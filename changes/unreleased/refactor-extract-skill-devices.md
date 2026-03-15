### Refactor

- **Extract `skill-devices` workspace crate**: moved DND focus-mode engine, composite EEG scores, and battery EMA from `muse_session.rs` (774 lines) into `crates/skill-devices/`. Zero Tauri dependencies. Includes 9 unit tests.
