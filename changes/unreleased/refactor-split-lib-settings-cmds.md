### Refactor

- **Split `lib.rs` and `settings_cmds/mod.rs`**: Extracted 5 new modules from the two largest files in `src-tauri`. `lib.rs` reduced from 1,580 to 1,413 lines by moving the macOS external renderer (172 lines) into `external_renderer.rs`. `settings_cmds/mod.rs` reduced from 1,560 to 938 lines by extracting device commands (`device_cmds.rs`), activity tracking (`activity_cmds.rs`), screenshot config/search (`screenshot_cmds.rs`), and skills management (`skills_cmds.rs`). All re-exports preserved — no changes to the public API or `generate_handler!` invocation.
