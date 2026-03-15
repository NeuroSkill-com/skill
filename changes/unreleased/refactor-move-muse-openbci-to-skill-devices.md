### Refactor

- **Move muse-rs and openbci dependencies to skill-devices crate**: Moved `muse-rs` and `openbci` dependency declarations from `src-tauri/Cargo.toml` to `crates/skill-devices/Cargo.toml` and re-exported them. Updated all imports in `muse_session.rs` and `openbci_session.rs` to use the re-exports via `skill_devices::`.
