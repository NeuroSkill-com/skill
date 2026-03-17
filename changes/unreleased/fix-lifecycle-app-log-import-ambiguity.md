### Bugfixes

- **Fix `app_log!` ambiguity in lifecycle module**: remove incorrect `app_log` item import from `src-tauri/src/lifecycle.rs` and clean unused Tauri imports in lifecycle/scanner modules so the Tauri crate compiles cleanly.
