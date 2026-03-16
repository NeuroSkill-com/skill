### Bugfixes

- **Fix unused import warnings in skill-router / src-tauri**: Removed ~30 unused `use` statements from `src-tauri/src/lib.rs`, gated `std::sync::Mutex` import in `state.rs` behind `#[cfg(not(feature = "llm"))]`, removed unused `CalibrationProfile` import from `helpers.rs`, and converted doc comment on macro invocation in `window_cmds.rs` to a regular comment to silence `unused_doc_comments` warning.
