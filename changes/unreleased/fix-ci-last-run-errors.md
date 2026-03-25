### Bugfixes

- **CI frontend format failures**: formatted Svelte files flagged by Biome in the `frontend-check` job (`LlmTab`, `ScreenshotsTab`, `LlmInferenceSection`, `ScreenshotPerformanceSection`).
- **Windows clippy failure**: removed an unnecessary raw-pointer cast in `src-tauri/src/skill_log.rs` when calling `SetStdHandle`, fixing `clippy::unnecessary_cast` under `-D warnings`.
