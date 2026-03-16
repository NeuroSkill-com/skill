### Refactor

- **`window_cmd!` / `window_tab_cmd!` macros for window commands**: Replaced 10 boilerplate `open_*_window` Tauri commands (each 7 lines) with 2–3 line macro invocations. New windows now require a single `window_cmd!` call instead of a full `#[tauri::command] pub async fn` definition.
