### Refactor

- **Extract `skill-tray` workspace crate**: moved progress-ring overlay, shortcut formatting, and dedup helpers from `tray.rs` (674 lines) into `crates/skill-tray/`. Pure `std`, zero dependencies. Includes 8 unit tests + 2 doc-tests.
