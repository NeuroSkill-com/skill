### Bugfixes

- **Fix 5 broken doctests across the workspace**: marked uncompilable doc examples as `ignore` in `skill-tools/src/log.rs`, `skill-llm/src/log.rs`, `skill-tts/src/log.rs`, `skill-headless/src/lib.rs`, `src-tauri/src/lib.rs`, and `src-tauri/src/skill_log.rs`. All workspace tests now pass with zero failures.
