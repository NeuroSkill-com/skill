### Refactor

- **Extract `skill-jobs` workspace crate**: moved sequential job queue (384 lines) into `crates/skill-jobs/`. Zero Tauri dependencies. All 3 unit tests pass.
